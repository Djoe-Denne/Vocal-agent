use axum::{
    extract::State,
    http::{header, StatusCode},
    response::Json,
};
use rustycog_command::CommandContext;
use rustycog_http::{AppState, ValidatedJson};

use orchestration_application::{TranscribeAudioCommand, TranscribeAudioRequest, TranscribeAudioResponse};

use crate::error::{error_mapper, HttpError};

pub async fn transcribe_audio(
    State(state): State<AppState>,
    ValidatedJson(request): ValidatedJson<TranscribeAudioRequest>,
) -> Result<(StatusCode, Json<TranscribeAudioResponse>), HttpError> {
    tracing::info!(
        sample_count = request.samples.len(),
        sample_rate_hz = request.sample_rate_hz.unwrap_or(0),
        language_hint = request.language_hint.as_deref().unwrap_or("auto"),
        session_id = request.session_id.as_deref().unwrap_or("auto"),
        "received transcribe request"
    );

    match execute_transcribe(&state, request).await {
        Ok(result) => {
            tracing::info!(
                segment_count = result.transcript.segments.len(),
                aligned_word_count = result.aligned_words.len(),
                has_tts_output = result.tts_output.is_some(),
                "transcribe request completed"
            );
            Ok((StatusCode::OK, Json(result)))
        }
        Err(error) => {
            tracing::error!(error = ?error, "transcribe request failed");
            Err(error)
        }
    }
}

pub async fn redub_audio_wav(
    State(state): State<AppState>,
    ValidatedJson(request): ValidatedJson<TranscribeAudioRequest>,
) -> Result<
    (
        StatusCode,
        [(header::HeaderName, &'static str); 2],
        Vec<u8>,
    ),
    HttpError,
> {
    tracing::info!(
        sample_count = request.samples.len(),
        sample_rate_hz = request.sample_rate_hz.unwrap_or(0),
        language_hint = request.language_hint.as_deref().unwrap_or("auto"),
        session_id = request.session_id.as_deref().unwrap_or("auto"),
        "received redub wav request"
    );

    let result = execute_transcribe(&state, request).await?;
    let (samples, sample_rate_hz) = if let Some(ref audio) = result.output_audio {
        (&audio.samples, audio.sample_rate_hz)
    } else if let Some(ref tts) = result.tts_output {
        (&tts.samples, tts.sample_rate_hz)
    } else {
        return Err(HttpError::Internal {
            message: "pipeline completed without output audio".to_string(),
        });
    };
    tracing::info!(
        sample_count = samples.len(),
        sample_rate_hz,
        "redub: encoding output audio"
    );
    let wav = encode_wav_f32_mono(samples, sample_rate_hz);

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "audio/wav"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"redub.wav\"",
            ),
        ],
        wav,
    ))
}

async fn execute_transcribe(
    state: &AppState,
    request: TranscribeAudioRequest,
) -> Result<TranscribeAudioResponse, HttpError> {
    let command = TranscribeAudioCommand::new(request);
    let context = CommandContext::new();
    state
        .command_service
        .execute(command, context)
        .await
        .map_err(error_mapper)
}

fn encode_wav_f32_mono(samples: &[f32], sample_rate_hz: u32) -> Vec<u8> {
    let channels: u16 = 1;
    let bits_per_sample: u16 = 32;
    let bytes_per_sample = (bits_per_sample / 8) as u32;
    let byte_rate = sample_rate_hz * channels as u32 * bytes_per_sample;
    let block_align = channels * (bits_per_sample / 8);
    let data_chunk_size = samples.len() as u32 * bytes_per_sample;
    let riff_chunk_size = 36 + data_chunk_size;

    let mut out = Vec::with_capacity((44 + data_chunk_size) as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_chunk_size.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&3u16.to_le_bytes()); // IEEE float
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate_hz.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits_per_sample.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_chunk_size.to_le_bytes());

    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}
