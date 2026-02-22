use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
pub struct WordTiming {
    pub word: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub language: LanguageTag,
    pub segments: Vec<TranscriptSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineContext {
    pub session_id: String,
    pub language_hint: Option<LanguageTag>,
    pub audio: AudioChunk,
    pub transcript: Option<Transcript>,
    pub aligned_words: Vec<WordTiming>,
    pub events: Vec<DomainEvent>,
    pub extensions: HashMap<String, Value>,
}

impl PipelineContext {
    pub fn new(session_id: impl Into<String>, language_hint: Option<LanguageTag>) -> Self {
        Self {
            session_id: session_id.into(),
            language_hint,
            audio: AudioChunk {
                sample_rate_hz: 16_000,
                samples: Vec::new(),
            },
            transcript: None,
            aligned_words: Vec::new(),
            events: Vec::new(),
            extensions: HashMap::new(),
        }
    }

    pub fn set_extension(&mut self, key: impl Into<String>, value: Value) -> Option<Value> {
        self.extensions.insert(key.into(), value)
    }

    pub fn extension(&self, key: &str) -> Option<&Value> {
        self.extensions.get(key)
    }

    pub fn extension_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.extensions.get_mut(key)
    }

    pub fn take_extension(&mut self, key: &str) -> Option<Value> {
        self.extensions.remove(key)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DomainEvent {
    FinalTranscript { transcript: Transcript },
    AlignmentUpdate { words: Vec<WordTiming> },
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

#[derive(Debug, Clone)]
pub struct AlignmentRequest {
    pub transcript: Transcript,
}

#[derive(Debug, Clone)]
pub struct AlignmentOutput {
    pub words: Vec<WordTiming>,
}
