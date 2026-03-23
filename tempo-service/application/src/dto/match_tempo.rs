use serde::{Deserialize, Serialize};
use validator::Validate;

use tempo_domain::WordTiming;

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct MatchTempoRequest {
    #[validate(length(min = 1))]
    pub tts_samples: Vec<f32>,
    #[validate(range(min = 8_000, max = 192_000))]
    pub tts_sample_rate_hz: Option<u32>,
    #[validate(length(min = 1))]
    pub original_timings: Vec<WordTiming>,
    #[validate(length(min = 1))]
    pub tts_timings: Vec<WordTiming>,
    #[validate(length(min = 1, max = 64))]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MatchTempoResponse {
    pub session_id: String,
    pub samples: Vec<f32>,
    pub sample_rate_hz: u32,
}
