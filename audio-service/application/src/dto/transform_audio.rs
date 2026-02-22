use serde::{Deserialize, Serialize};
use validator::Validate;

use audio_domain::TransformMetadata;

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct TransformAudioRequest {
    #[validate(length(min = 1))]
    pub samples: Vec<f32>,
    #[validate(range(min = 8_000, max = 192_000))]
    pub sample_rate_hz: Option<u32>,
    #[validate(range(min = 8_000, max = 192_000))]
    pub target_sample_rate_hz: Option<u32>,
    #[validate(length(min = 1, max = 64))]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransformAudioResponse {
    pub session_id: String,
    pub samples: Vec<f32>,
    pub sample_rate_hz: u32,
    pub metadata: TransformMetadata,
}
