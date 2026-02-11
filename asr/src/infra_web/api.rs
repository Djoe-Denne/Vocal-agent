//! Web API adapter for ASR.
//!
//! Axum-based REST endpoints for audio transcription.
//!
//! Endpoints:
//! - `POST /transcribe` — multipart audio upload, returns JSON transcript
//! - `GET  /health`     — health check

use std::sync::{Arc, Mutex};

use axum::extract::{DefaultBodyLimit, Multipart, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;

use crate::application::use_cases::TranscribeAudioUseCase;
use crate::domain::value_objects::Language;

// ---------------------------------------------------------------------------
// Shared application state
// ---------------------------------------------------------------------------

/// Shared state for the Axum application.
///
/// Uses `std::sync::Mutex` (not tokio) so the guard can be held inside
/// `spawn_blocking` which requires `Send + 'static`.
pub struct AppState {
    pub use_case: Mutex<TranscribeAudioUseCase>,
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct TranscribeResponse {
    text: String,
    timings: TimingsResponse,
    warnings: Vec<String>,
}

#[derive(Serialize)]
struct TimingsResponse {
    model_load_ms: f64,
    preprocess_ms: f64,
    inference_ms: f64,
    postprocess_ms: f64,
    total_ms: f64,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the Axum router with shared state.
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/transcribe", post(transcribe))
        .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

/// `POST /transcribe`
///
/// Accepts multipart form data with:
/// - `file` (required) — the audio file
/// - `language` (optional) — language hint, defaults to config default
async fn transcribe(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    let mut audio_bytes: Option<Vec<u8>> = None;
    let mut language: Option<Language> = None;
    let mut filename = String::from("upload.wav");

    // Parse multipart fields.
    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                if let Some(fname) = field.file_name() {
                    filename = fname.to_string();
                }
                audio_bytes = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| {
                            (
                                StatusCode::BAD_REQUEST,
                                Json(ErrorResponse {
                                    error: format!("Failed to read file field: {e}"),
                                }),
                            )
                        })?
                        .to_vec(),
                );
            }
            "language" => {
                let text = field.text().await.map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            error: format!("Failed to read language field: {e}"),
                        }),
                    )
                })?;
                if !text.is_empty() {
                    language = Some(text.parse::<Language>().map_err(|e| {
                        (
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse {
                                error: format!("Invalid language: {e}"),
                            }),
                        )
                    })?);
                }
            }
            _ => {
                // Ignore unknown fields.
            }
        }
    }

    let audio_bytes = audio_bytes.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Missing required 'file' field in multipart form".to_string(),
            }),
        )
    })?;

    // Write audio to a temporary file.
    let temp_dir = std::env::temp_dir().join("asr_uploads");
    std::fs::create_dir_all(&temp_dir).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to create temp directory: {e}"),
            }),
        )
    })?;

    let temp_path = temp_dir.join(format!(
        "{}_{filename}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    ));

    std::fs::write(&temp_path, &audio_bytes).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to write temp file: {e}"),
            }),
        )
    })?;

    // Run transcription in a blocking task (the ASR engine is synchronous).
    let result = tokio::task::spawn_blocking(move || {
        let mut use_case = state
            .use_case
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        let request = use_case.build_request(temp_path.clone(), None, language);
        let result = use_case.execute(request);

        // Clean up temp file (best-effort).
        let _ = std::fs::remove_file(&temp_path);

        result
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Task join error: {e}"),
            }),
        )
    })?
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Transcription failed: {e}"),
            }),
        )
    })?;

    Ok(Json(TranscribeResponse {
        text: result.text,
        timings: TimingsResponse {
            model_load_ms: result.timings.model_load_ms,
            preprocess_ms: result.timings.preprocess_ms,
            inference_ms: result.timings.inference_ms,
            postprocess_ms: result.timings.postprocess_ms,
            total_ms: result.timings.total_ms,
        },
        warnings: result.warnings,
    }))
}
