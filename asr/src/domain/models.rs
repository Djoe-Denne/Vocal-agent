//! Domain entities and aggregates for ASR.
//!
//! Contains the core business data structures: requests, results,
//! resolved models, transcription options, and timing information.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::value_objects::{Language, ModelRef, SampleRate};

// ---------------------------------------------------------------------------
// ResolvedModel — model ready for engine consumption
// ---------------------------------------------------------------------------

/// A model that has been resolved to local paths, regardless of its
/// original source (local directory or future HuggingFace download).
/// The engine never sees the model source — only this resolved form.
#[derive(Debug, Clone)]
pub struct ResolvedModel {
    /// Root directory containing model weights and config.
    pub root_dir: PathBuf,
    /// Additional metadata (optional).
    pub metadata: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// TranscriptionOptions — generation parameters
// ---------------------------------------------------------------------------

/// Transcription parameters.
///
/// Domain-owned — future-proofed for engine-specific settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionOptions {
    /// Language hint for transcription.
    #[serde(default)]
    pub language: Language,
}

impl Default for TranscriptionOptions {
    fn default() -> Self {
        Self {
            language: Language::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// TranscriptionRequest — what the caller wants
// ---------------------------------------------------------------------------

/// A fully-described transcription request. Built by merging config defaults
/// with CLI/API overrides.
#[derive(Debug, Clone)]
pub struct TranscriptionRequest {
    /// Path to the audio file to transcribe.
    pub audio_path: PathBuf,
    /// Where to find the model.
    pub model_ref: ModelRef,
    /// Transcription options.
    pub options: TranscriptionOptions,
    /// Pipeline stage names for pre-processing.
    pub pre_stages: Vec<String>,
    /// Pipeline stage names for post-processing.
    pub post_stages: Vec<String>,
}

// ---------------------------------------------------------------------------
// TranscriptionTiming — per-phase timing breakdown
// ---------------------------------------------------------------------------

/// Timing breakdown for a transcription operation.
#[derive(Debug, Clone, Default, Serialize)]
pub struct TranscriptionTiming {
    /// Time to load/prepare the model (ms).
    pub model_load_ms: f64,
    /// Time spent in pre-processors (ms).
    pub preprocess_ms: f64,
    /// Time spent in model inference (ms).
    pub inference_ms: f64,
    /// Time spent in post-processors (ms).
    pub postprocess_ms: f64,
    /// Total wall-clock time (ms).
    pub total_ms: f64,
}

// ---------------------------------------------------------------------------
// TranscriptionResult — what comes back
// ---------------------------------------------------------------------------

/// Result of a transcription operation.
#[derive(Debug, Clone, Serialize)]
pub struct TranscriptionResult {
    /// Transcribed text.
    pub text: String,
    /// Sample rate of the source audio (if known).
    pub sample_rate: Option<SampleRate>,
    /// Duration of the source audio in seconds (if known).
    pub audio_duration_secs: Option<f64>,
    /// Metadata bag (model info, engine details, etc.).
    pub metadata: HashMap<String, String>,
    /// Per-phase timing breakdown.
    pub timings: TranscriptionTiming,
    /// Non-fatal warnings.
    pub warnings: Vec<String>,
}
