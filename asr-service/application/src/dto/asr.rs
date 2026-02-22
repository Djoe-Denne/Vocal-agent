use serde::{Deserialize, Serialize};
use validator::Validate;

use asr_domain::Transcript;

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct TranscribeAudioRequest {
    #[validate(length(min = 1))]
    pub samples: Vec<f32>,
    #[validate(range(min = 8_000, max = 192_000))]
    pub sample_rate_hz: Option<u32>,
    #[validate(length(min = 1, max = 16))]
    pub language_hint: Option<String>,
    #[validate(length(min = 1, max = 64))]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TranscribeAudioResponse {
    pub session_id: String,
    pub transcript: Transcript,
    pub text: String,
}
