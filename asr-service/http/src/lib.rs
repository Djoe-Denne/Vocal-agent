use axum::{extract::DefaultBodyLimit, routing::post};
use rustycog_config::ServerConfig;
use rustycog_http::{AppState, RouteBuilder};

pub mod error;
pub mod handlers;

pub use error::{error_mapper, HttpError};
pub use handlers::*;

pub async fn create_app_routes(state: AppState, config: ServerConfig) -> anyhow::Result<()> {
    // WAV payloads serialized as float arrays can be large; raise route body limit.
    let transcribe_route = post(transcribe_audio).layer(DefaultBodyLimit::max(64 * 1024 * 1024));

    RouteBuilder::new(state)
        .health_check()
        .route("/api/asr/transcribe", transcribe_route)
        .build(config)
        .await
}
