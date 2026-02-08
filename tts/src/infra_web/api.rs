//! Web API adapter for TTS.
//!
//! Axum-based REST endpoints for speech synthesis.
//!
//! Endpoints:
//! - `POST /synthesize` — JSON request body, returns WAV audio binary
//! - `GET  /health`     — health check

use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::application::use_cases::SynthesizeSpeechUseCase;
use crate::domain::models::SynthesisOptions;
use crate::domain::value_objects::{Language, VoiceId};

// ---------------------------------------------------------------------------
// Shared application state
// ---------------------------------------------------------------------------

/// Shared state for the Axum application.
///
/// Uses `std::sync::Mutex` (not tokio) so the guard can be held inside
/// `spawn_blocking` which requires `Send + 'static`.
pub struct AppState {
    pub use_case: Mutex<SynthesizeSpeechUseCase>,
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SynthesizeRequest {
    /// Text to synthesise.
    text: String,
    /// Voice to use (preset name or custom identifier).
    #[serde(default = "default_voice")]
    voice: String,
    /// Target language.
    #[serde(default = "default_language")]
    language: String,
    /// Voice design instruction text (VoiceDesign models only).
    instruct: Option<String>,
    /// Synthesis options.
    #[serde(default)]
    options: Option<SynthesisOptions>,
}

fn default_voice() -> String {
    "ryan".to_owned()
}

fn default_language() -> String {
    "english".to_owned()
}

#[derive(Serialize)]
struct SynthesizeMetadata {
    sample_rate: u32,
    duration_secs: f64,
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
        .route("/synthesize", post(synthesize))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

/// `POST /synthesize`
///
/// Accepts a JSON body with `text`, `voice`, `language`, optional `instruct`
/// and `options`. Returns WAV audio with metadata in response headers.
async fn synthesize(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SynthesizeRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    let voice: VoiceId = body.voice.parse().map_err(|e: anyhow::Error| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("Invalid voice: {e}"),
            }),
        )
    })?;

    let language: Language = body.language.parse().map_err(|e: anyhow::Error| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("Invalid language: {e}"),
            }),
        )
    })?;

    // Run synthesis in a blocking task (the TTS engine is synchronous).
    let result = tokio::task::spawn_blocking(move || {
        let mut use_case = state
            .use_case
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        let request = use_case.build_request(
            body.text,
            None, // model_ref from config
            Some(voice),
            Some(language),
            body.options,
            body.instruct,
            None, // ref_audio_path
            None, // ref_text
        );

        use_case.execute(request)
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
                error: format!("Synthesis failed: {e}"),
            }),
        )
    })?;

    // Encode audio as WAV in memory.
    let sample_rate = result.sample_rate.0;
    let num_samples = result.audio_samples.len();
    let duration_secs = num_samples as f64 / sample_rate as f64;

    let wav_bytes = encode_wav(&result.audio_samples, sample_rate).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("WAV encoding failed: {e}"),
            }),
        )
    })?;

    // Build metadata JSON for the header.
    let metadata = SynthesizeMetadata {
        sample_rate,
        duration_secs,
        timings: TimingsResponse {
            model_load_ms: result.timings.model_load_ms,
            preprocess_ms: result.timings.preprocess_ms,
            inference_ms: result.timings.inference_ms,
            postprocess_ms: result.timings.postprocess_ms,
            total_ms: result.timings.total_ms,
        },
        warnings: result.warnings,
    };

    let metadata_json = serde_json::to_string(&metadata).unwrap_or_default();

    let metadata_header = header::HeaderValue::from_str(&metadata_json)
        .unwrap_or_else(|_| header::HeaderValue::from_static("{}"));

    Ok((
        [
            (header::CONTENT_TYPE, header::HeaderValue::from_static("audio/wav")),
            (header::HeaderName::from_static("x-tts-metadata"), metadata_header),
        ],
        wav_bytes,
    ))
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
