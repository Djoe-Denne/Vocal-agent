//! Shared domain: universal pipeline stage contract.
//!
//! Defines the [`Stage`] trait, [`PipelineContext`] struct, and [`MediaType`] enum
//! that every pipeline stage -- regardless of module -- must adhere to.

use std::collections::HashMap;

/// The kind of payload a stage consumes or produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    /// Raw audio data (samples + sample_rate).
    Audio,
    /// UTF-8 text.
    Text,
}

/// Mutable bag of data that flows through every stage in a pipeline.
///
/// Stages read the fields they need and write back their results.
#[derive(Debug, Clone)]
pub struct PipelineContext {
    /// Primary audio payload (PCM f32 samples).
    pub audio: Option<Vec<f32>>,
    /// Sample rate of the audio payload.
    pub sample_rate: u32,
    /// Primary text payload.
    pub text: String,
    /// Metadata bag -- stages can stash anything here.
    pub meta: HashMap<String, String>,
    /// Accumulated diagnostics / timing per stage.
    pub stage_results: Vec<StageResult>,
}

/// Timing result for a single stage execution.
#[derive(Debug, Clone)]
pub struct StageResult {
    pub stage: String,
    pub elapsed_secs: f64,
}

impl Default for PipelineContext {
    fn default() -> Self {
        Self {
            audio: None,
            sample_rate: 16_000,
            text: String::new(),
            meta: HashMap::new(),
            stage_results: Vec::new(),
        }
    }
}

impl PipelineContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Default::default()
        }
    }

    pub fn with_audio(audio: Vec<f32>, sample_rate: u32) -> Self {
        Self {
            audio: Some(audio),
            sample_rate,
            ..Default::default()
        }
    }
}

/// Single processing step in a pipeline.
///
/// Every pre-processor, model, and post-processor implements this.
pub trait Stage: Send + Sync {
    /// Unique identifier used in config (e.g. "vad", "whisper", "cleanup").
    fn name(&self) -> &str;

    /// What this stage consumes. Default: [`MediaType::Text`].
    fn input_type(&self) -> MediaType {
        MediaType::Text
    }

    /// What this stage produces. Default: same as [`Stage::input_type`].
    fn output_type(&self) -> MediaType {
        self.input_type()
    }

    /// Transform the context in-place and return it.
    fn process(&self, ctx: PipelineContext) -> anyhow::Result<PipelineContext>;

    /// Optional: load heavy resources (models, etc.).
    fn load(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Optional: free resources.
    fn unload(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}
