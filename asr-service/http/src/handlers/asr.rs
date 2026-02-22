use axum::{extract::State, http::StatusCode, response::Json};
use rustycog_command::CommandContext;
use rustycog_http::{AppState, ValidatedJson};

use asr_application::{TranscribeAudioCommand, TranscribeAudioRequest, TranscribeAudioResponse};

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

    let command = TranscribeAudioCommand::new(request);
    let context = CommandContext::new();
    let command_result = state.command_service.execute(command, context).await;

    match command_result {
        Ok(result) => {
            tracing::info!(
                segment_count = result.transcript.segments.len(),
                aligned_word_count = result.aligned_words.len(),
                "transcribe request completed"
            );
            Ok((StatusCode::OK, Json(result)))
        }
        Err(error) => {
            tracing::error!(error = %error, "transcribe request failed");
            Err(error_mapper(error))
        }
    }
}
