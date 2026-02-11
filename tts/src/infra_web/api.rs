//! Web API adapter for TTS.
//!
//! Axum-based REST endpoints aligned with the Python service.
//!
//! Endpoints:
//! - `POST /v1/audio/speech` — OpenAI-style request body, returns WAV audio
//! - `GET  /v1/audio/voices` — speakers/languages and voice-sample layout
//! - `GET  /health`          — health check

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::application::use_cases::SynthesizeSpeechUseCase;
use crate::domain::models::SynthesisOptions;
use crate::domain::value_objects::VoiceId;

// ---------------------------------------------------------------------------
// Shared application state
// ---------------------------------------------------------------------------

/// Shared state for the Axum application.
///
/// Uses `std::sync::Mutex` (not tokio) so the guard can be held inside
/// `spawn_blocking` which requires `Send + 'static`.
pub struct AppState {
    pub use_case: Mutex<SynthesizeSpeechUseCase>,
    pub voices_dir: PathBuf,
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SpeechRequest {
    /// Text to synthesise.
    input: String,
    /// Voice sample ID under `voices/` (e.g. "justamon").
    voice_sample: Option<String>,
    /// Built-in speaker preset (e.g. "Serena", "Vivian").
    voice_preset: Option<String>,
    /// Guidance text for voice output.
    guidance: Option<String>,
    /// Named pipeline override.
    pipeline: Option<String>,
}

#[derive(Serialize)]
struct ErrorResponse {
    detail: String,
}

#[derive(Serialize)]
struct VoiceSampleLayout {
    example: String,
    audio: Vec<String>,
    text: String,
}

#[derive(Serialize)]
struct VoicesResponse {
    speakers: Vec<String>,
    languages: Vec<String>,
    voice_samples_dir: String,
    voice_sample_layout: VoiceSampleLayout,
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the Axum router with shared state.
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/audio/speech", post(create_speech))
        .route("/v1/audio/voices", get(list_voices))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

/// `POST /v1/audio/speech`
async fn create_speech(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SpeechRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    if body.input.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                detail: "input must not be empty.".to_owned(),
            }),
        ));
    }

    if body.pipeline.is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                detail: "pipeline overrides are not supported by this Rust service.".to_owned(),
            }),
        ));
    }

    if body.voice_sample.is_none() && body.voice_preset.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                detail: "Either voice_sample or voice_preset must be provided.".to_owned(),
            }),
        ));
    }

    let mut voice: Option<VoiceId> = None;
    let mut ref_audio_path: Option<PathBuf> = None;
    let mut ref_text: Option<String> = None;

    if let Some(sample_id) = body.voice_sample.clone() {
        let (audio_path, sample_text) = resolve_voice_sample(&state.voices_dir, &sample_id).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    detail: e.to_string(),
                }),
            )
        })?;
        voice = Some(VoiceId::Custom(sample_id));
        ref_audio_path = Some(audio_path);
        ref_text = sample_text;
    }

    if let Some(voice_preset) = body.voice_preset {
        if voice.is_none() {
            let parsed_voice: VoiceId = voice_preset.parse().map_err(|e: anyhow::Error| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        detail: format!("Invalid voice_preset: {e}"),
                    }),
                )
            })?;
            voice = Some(parsed_voice);
        }
    }

    // Run synthesis in a blocking task (the TTS engine is synchronous).
    let result = tokio::task::spawn_blocking(move || {
        let mut use_case = state
            .use_case
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        let request = use_case.build_request(
            body.input,
            None, // model_ref from config
            Some(voice.unwrap_or_default()),
            None, // language from config default
            Some(SynthesisOptions::default()),
            body.guidance,
            ref_audio_path,
            ref_text,
        );

        use_case.execute(request)
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                detail: format!("Task join error: {e}"),
            }),
        )
    })?
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                detail: format!("Synthesis failed: {e}"),
            }),
        )
    })?;

    // Encode audio as WAV in memory.
    let sample_rate = result.sample_rate.0;

    let wav_bytes = encode_wav(&result.audio_samples, sample_rate).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                detail: format!("WAV encoding failed: {e}"),
            }),
        )
    })?;

    Ok((
        [(header::CONTENT_TYPE, header::HeaderValue::from_static("audio/wav"))],
        wav_bytes,
    ))
}

/// `GET /v1/audio/voices`
async fn list_voices(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(VoicesResponse {
        speakers: vec![
            "Serena".to_owned(),
            "Vivian".to_owned(),
            "UncleFu".to_owned(),
            "Ryan".to_owned(),
            "Aiden".to_owned(),
            "OnoAnna".to_owned(),
            "Sohee".to_owned(),
            "Eric".to_owned(),
            "Dylan".to_owned(),
        ],
        languages: vec![
            "english".to_owned(),
            "chinese".to_owned(),
            "japanese".to_owned(),
            "korean".to_owned(),
            "german".to_owned(),
            "french".to_owned(),
            "russian".to_owned(),
            "portuguese".to_owned(),
            "spanish".to_owned(),
            "italian".to_owned(),
        ],
        voice_samples_dir: state.voices_dir.to_string_lossy().to_string(),
        voice_sample_layout: VoiceSampleLayout {
            example: "voices/my_voice/".to_owned(),
            audio: vec![
                "audio.wav".to_owned(),
                "audio.flac".to_owned(),
                "audio.ogg".to_owned(),
                "audio.mp3".to_owned(),
            ],
            text: "text.txt (optional)".to_owned(),
        },
    })
}

fn resolve_voice_sample(voices_dir: &Path, sample_id: &str) -> anyhow::Result<(PathBuf, Option<String>)> {
    let sample_dir = voices_dir.join(sample_id);
    if !sample_dir.exists() {
        anyhow::bail!(
            "Voice sample '{}' not found in {}.",
            sample_id,
            voices_dir.display()
        );
    }

    let audio_candidates = ["audio.wav", "audio.flac", "audio.ogg", "audio.mp3"];
    let audio_path = audio_candidates
        .iter()
        .map(|name| sample_dir.join(name))
        .find(|candidate| candidate.exists())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Voice sample '{}' is missing audio.wav (or flac/ogg/mp3).",
                sample_id
            )
        })?;

    let text_path = sample_dir.join("text.txt");
    let text_value = if text_path.exists() {
        let raw = fs::read_to_string(text_path)?;
        Some(raw.trim().to_owned())
    } else {
        None
    };

    Ok((audio_path, text_value))
}

// ---------------------------------------------------------------------------
// WAV encoding helper
// ---------------------------------------------------------------------------

/// Encode PCM f32 samples as a WAV byte buffer (PCM16, mono).
fn encode_wav(samples: &[f32], sample_rate: u32) -> anyhow::Result<Vec<u8>> {
    let num_samples = samples.len();
    let bits_per_sample: u16 = 16;
    let num_channels: u16 = 1;
    let byte_rate = sample_rate * u32::from(num_channels) * u32::from(bits_per_sample) / 8;
    let block_align = num_channels * bits_per_sample / 8;
    let data_size = num_samples as u32 * u32::from(block_align);

    let mut buf = Vec::with_capacity(44 + data_size as usize);

    // RIFF header
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_size).to_le_bytes());
    buf.extend_from_slice(b"WAVE");

    // fmt chunk
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    buf.extend_from_slice(&num_channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&bits_per_sample.to_le_bytes());

    // data chunk
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());

    for &sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let pcm16 = (clamped * 32767.0) as i16;
        buf.extend_from_slice(&pcm16.to_le_bytes());
    }

    Ok(buf)
}
