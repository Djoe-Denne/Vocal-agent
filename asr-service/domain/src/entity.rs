use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LanguageTag {
    Fr,
    En,
    Auto,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioChunk {
    pub sample_rate_hz: u32,
    pub samples: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptToken {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub tokens: Vec<TranscriptToken>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub language: LanguageTag,
    pub segments: Vec<TranscriptSegment>,
}

#[derive(Debug, Clone)]
pub struct TranscriptionRequest {
    pub language_hint: Option<LanguageTag>,
    pub audio: AudioChunk,
}

#[derive(Debug, Clone)]
pub struct TranscriptionOutput {
    pub transcript: Transcript,
}
