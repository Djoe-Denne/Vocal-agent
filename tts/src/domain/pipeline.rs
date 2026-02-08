//! Domain pipeline contexts for TTS pre/post processing.
//!
//! Typed contexts that flow through pre-processor and post-processor chains.
//! These are distinct from the shared crate's generic `PipelineContext`
//! because TTS processing has domain-specific fields.

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// PreProcessorContext
// ---------------------------------------------------------------------------

/// Mutable context flowing through the pre-processor chain.
///
/// Pre-processors can modify the text, add artifacts, or emit warnings
/// before the text reaches the TTS engine.
#[derive(Debug, Clone)]
pub struct PreProcessorContext {
    /// Unique request identifier for tracing.
    pub request_id: String,
    /// The text to be synthesised (may be modified by processors).
    pub text: String,
    /// Named artifacts produced by processors (e.g. detected language,
    /// normalised tokens, chunk boundaries).
    pub artifacts: HashMap<String, String>,
    /// Accumulated warnings from processors.
    pub warnings: Vec<String>,
}

impl PreProcessorContext {
    /// Create a new context for the given text.
    pub fn new(request_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            request_id: request_id.into(),
            text: text.into(),
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
/// Post-processors can modify the audio buffer, adjust metadata,
/// or emit warnings after synthesis.
#[derive(Debug, Clone)]
pub struct PostProcessorContext {
    /// Unique request identifier for tracing.
    pub request_id: String,
    /// The synthesised audio samples (PCM f32).
    pub audio_samples: Vec<f32>,
    /// Sample rate of the audio.
    pub sample_rate: u32,
    /// Duration of the audio in seconds.
    pub audio_duration_secs: f64,
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
    /// Create a new context from synthesis output.
    pub fn new(
        request_id: impl Into<String>,
        audio_samples: Vec<f32>,
        sample_rate: u32,
    ) -> Self {
        let duration = if sample_rate > 0 {
            audio_samples.len() as f64 / sample_rate as f64
        } else {
            0.0
        };
        Self {
            request_id: request_id.into(),
            audio_samples,
            sample_rate,
            audio_duration_secs: duration,
            artifacts: HashMap::new(),
            warnings: Vec::new(),
            stage_timings: Vec::new(),
        }
    }
}
