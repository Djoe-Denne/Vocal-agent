use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordTiming {
    pub word: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub confidence: f32,
}

#[derive(Debug, Clone)]
pub struct TempoMatchRequest {
    pub tts_samples: Vec<f32>,
    pub tts_sample_rate_hz: u32,
    pub original_timings: Vec<WordTiming>,
    pub tts_timings: Vec<WordTiming>,
}

#[derive(Debug, Clone)]
pub struct TempoMatchOutput {
    pub samples: Vec<f32>,
    pub sample_rate_hz: u32,
}
