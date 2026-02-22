use serde::{Deserialize, Serialize};
use validator::Validate;

use alignment_domain::{Transcript, WordTiming};

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct EnrichTranscriptRequest {
    #[validate(length(min = 1))]
    pub samples: Vec<f32>,
    #[validate(range(min = 8_000, max = 192_000))]
    pub sample_rate_hz: Option<u32>,
    pub transcript: Transcript,
    #[validate(length(min = 1, max = 64))]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnrichTranscriptResponse {
    pub session_id: String,
    pub transcript: Transcript,
    pub aligned_words: Vec<WordTiming>,
    pub text: String,
}
