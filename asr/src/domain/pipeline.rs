//! Domain pipeline contexts for ASR pre/post processing.
//!
//! Typed contexts that flow through pre-processor and post-processor chains.
//! These are distinct from the shared crate's generic `PipelineContext`
//! because ASR processing has domain-specific fields.

use std::collections::HashMap;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// PreProcessorContext
// ---------------------------------------------------------------------------

/// Mutable context flowing through the pre-processor chain.
///
/// Pre-processors can modify the audio path (e.g. after resampling to a
/// temporary file), add artifacts, or emit warnings before the audio
/// reaches the ASR engine.
#[derive(Debug, Clone)]
pub struct PreProcessorContext {
    /// Unique request identifier for tracing.
    pub request_id: String,
    /// Path to the audio file (may be replaced by processors, e.g.
    /// after resampling to a temp file).
    pub audio_path: PathBuf,
    /// Named artifacts produced by processors (e.g. detected sample rate,
    /// silence boundaries, VAD segments).
    pub artifacts: HashMap<String, String>,
    /// Accumulated warnings from processors.
    pub warnings: Vec<String>,
}

impl PreProcessorContext {
    /// Create a new context for the given audio path.
    pub fn new(request_id: impl Into<String>, audio_path: impl Into<PathBuf>) -> Self {
        Self {
            request_id: request_id.into(),
            audio_path: audio_path.into(),
            artifacts: HashMap::new(),
            warnings: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// PostProcessorContext
// ---------------------------------------------------------------------------

/// Mutable context flowing through the post-processor chain.
///
/// Post-processors can modify the transcribed text, adjust metadata,
/// or emit warnings after transcription.
#[derive(Debug, Clone)]
pub struct PostProcessorContext {
    /// Unique request identifier for tracing.
    pub request_id: String,
    /// The transcribed text (may be modified by processors).
    pub text: String,
    /// Named artifacts (carried forward from pre-processing + new ones).
    pub artifacts: HashMap<String, String>,
    /// Accumulated warnings.
    pub warnings: Vec<String>,
    /// Per-stage timing records.
    pub stage_timings: Vec<StageTiming>,
}

/// Timing record for a single pipeline stage.
#[derive(Debug, Clone)]
pub struct StageTiming {
    pub stage_name: String,
    pub elapsed_ms: f64,
}

impl PostProcessorContext {
    /// Create a new context from transcription output.
    pub fn new(request_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            request_id: request_id.into(),
            text: text.into(),
            artifacts: HashMap::new(),
            warnings: Vec::new(),
            stage_timings: Vec::new(),
        }
    }
}
