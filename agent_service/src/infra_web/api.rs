use std::sync::{Arc, Mutex};

use axum::extract::{DefaultBodyLimit, Multipart, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;

use crate::application::use_cases::ProcessAudioUseCase;

pub struct AppState {
    pub use_case: Mutex<ProcessAudioUseCase>,
}

#[derive(Serialize)]
struct ProcessResponse {
    transcription: String,
    agent_response: Option<String>,
    warnings: Vec<String>,
    timings: TimingsResponse,
}

#[derive(Serialize)]
struct TimingsResponse {
    asr_ms: f64,
    agent_ms: f64,
    tts_ms: f64,
    total_ms: f64,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/process", post(process))
        .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
        .with_state(state)
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn process(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<axum::response::Response, (StatusCode, Json<ErrorResponse>)> {
    eprintln!("[agent_service][api] /process called");
    let mut audio_bytes: Option<Vec<u8>> = None;
    let mut language: Option<String> = None;
    let mut filename = String::from("upload.wav");

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
                    language = Some(text);
                }
            }
            _ => {}
        }
    }

    let audio_bytes = audio_bytes.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Missing required 'file' field in multipart form".to_owned(),
            }),
        )
    })?;
    eprintln!(
        "[agent_service][api] multipart parsed filename={} audio_bytes={} language={:?}",
        filename,
        audio_bytes.len(),
        language
    );

    let temp_dir = std::env::temp_dir().join("agent_service_uploads");
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

    let result = tokio::task::spawn_blocking(move || {
        let mut use_case = state
            .use_case
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        let result = use_case.execute(temp_path.clone(), language);
        let _ = std::fs::remove_file(&temp_path);
        result
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Task join error: {e:#}"),
            }),
        )
    })?
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Processing failed: {e:#}"),
            }),
        )
    })?;

    // When TTS produced audio, return raw audio bytes with metadata in headers.
    // Otherwise fall back to the JSON response for backward compatibility.
    if let Some(audio) = result.audio {
        eprintln!(
            "[agent_service][api] returning audio content_type={} bytes={} timings(asr={:.1},agent={:.1},tts={:.1},total={:.1})",
            audio.content_type,
            audio.audio_data.len(),
            result.timings.asr_ms,
            result.timings.agent_ms,
            result.timings.tts_ms,
            result.timings.total_ms
        );
        let mut headers = HeaderMap::new();

        if let Ok(ct) = HeaderValue::from_str(&audio.content_type) {
            headers.insert("content-type", ct);
        } else {
            headers.insert("content-type", HeaderValue::from_static("audio/wav"));
        }

        if let Ok(v) = HeaderValue::from_str(&result.transcription) {
            headers.insert("x-transcription", v);
        }
        if let Some(agent_text) = &result.agent_response {
            if let Ok(v) = HeaderValue::from_str(agent_text) {
                headers.insert("x-agent-response", v);
            }
        }
        if let Ok(v) = HeaderValue::from_str(&format!("{:.1}", result.timings.asr_ms)) {
            headers.insert("x-timing-asr-ms", v);
        }
        if let Ok(v) = HeaderValue::from_str(&format!("{:.1}", result.timings.agent_ms)) {
            headers.insert("x-timing-agent-ms", v);
        }
        if let Ok(v) = HeaderValue::from_str(&format!("{:.1}", result.timings.tts_ms)) {
            headers.insert("x-timing-tts-ms", v);
        }
        if let Ok(v) = HeaderValue::from_str(&format!("{:.1}", result.timings.total_ms)) {
            headers.insert("x-timing-total-ms", v);
        }
        if let Ok(warnings_json) = serde_json::to_string(&result.warnings) {
            if let Ok(v) = HeaderValue::from_str(&warnings_json) {
                headers.insert("x-warnings", v);
            }
        }

        Ok((headers, audio.audio_data).into_response())
    } else {
        eprintln!(
            "[agent_service][api] returning json transcription_len={} agent_len={} warnings={}",
            result.transcription.len(),
            result.agent_response.as_ref().map(|s| s.len()).unwrap_or(0),
            result.warnings.len()
        );
        Ok(Json(ProcessResponse {
            transcription: result.transcription,
            agent_response: result.agent_response,
            warnings: result.warnings,
            timings: TimingsResponse {
                asr_ms: result.timings.asr_ms,
                agent_ms: result.timings.agent_ms,
                tts_ms: result.timings.tts_ms,
                total_ms: result.timings.total_ms,
            },
        })
        .into_response())
    }
}
