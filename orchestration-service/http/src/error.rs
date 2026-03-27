use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use rustycog_command::CommandError;
use serde_json::json;

#[derive(Debug)]
pub enum HttpError {
    Validation { message: String },
    Unauthorized,
    Forbidden,
    NotFound,
    Internal { message: String },
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            HttpError::Validation { message } => (StatusCode::UNPROCESSABLE_ENTITY, message),
            HttpError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized".to_string()),
            HttpError::Forbidden => (StatusCode::FORBIDDEN, "Forbidden".to_string()),
            HttpError::NotFound => (StatusCode::NOT_FOUND, "Not found".to_string()),
            HttpError::Internal { message } => (StatusCode::INTERNAL_SERVER_ERROR, message),
        };

        (
            status,
            Json(json!({
                "error": message,
            })),
        )
            .into_response()
    }
}

pub fn error_mapper(error: CommandError) -> HttpError {
    match error {
        CommandError::Validation { .. } => HttpError::Validation {
            message: error.to_string(),
        },
        CommandError::Authentication { .. } => HttpError::Unauthorized,
        CommandError::Business { .. } => HttpError::Validation {
            message: error.to_string(),
        },
        _ => HttpError::Internal {
            message: error.to_string(),
        },
    }
}
