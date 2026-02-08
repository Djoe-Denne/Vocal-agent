//! Domain entities and aggregates for TTS.
//!
//! Contains the core business data structures: requests, results,
//! resolved models, synthesis options, and timing information.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::value_objects::{Language, ModelRef, SampleRate, VoiceId};

// ---------------------------------------------------------------------------
// ModelVariant — detected model type
// ---------------------------------------------------------------------------

/// The variant of a Qwen3-TTS model, determining which synthesis methods
/// are valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelVariant {
    /// Supports voice cloning from reference audio (ICL or x-vector).
    Base,
    /// Supports 9 preset speakers.
    CustomVoice,
    /// Supports text-described voice design.
    VoiceDesign,
}

impl std::fmt::Display for ModelVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Base => f.write_str("Base"),
            Self::CustomVoice => f.write_str("CustomVoice"),
            Self::VoiceDesign => f.write_str("VoiceDesign"),
        }
    }
}

// ---------------------------------------------------------------------------
// ResolvedModel — model ready for engine consumption
// ---------------------------------------------------------------------------

/// A model that has been resolved to local paths, regardless of its
/// original source (HuggingFace or local). The engine never sees the
/// model source — only this resolved form.
#[derive(Debug, Clone)]
pub struct ResolvedModel {
    /// Root directory containing model weights and config.
    ///
    /// For local models this is the model directory itself.
    /// For HF-downloaded models this is a logical identifier (the actual
    /// files are referenced via `files`).
    pub root_dir: PathBuf,
    /// Detected model variant.
    pub variant: ModelVariant,
    /// Explicit file paths for individually-downloaded model files.
    ///
    /// When `Some`, the engine loads from these paths directly.
    /// When `None`, the engine loads from `root_dir` as a self-contained
    /// model directory.
    pub files: Option<ResolvedModelFiles>,
    /// Additional metadata from `config.json` (optional).
    pub metadata: HashMap<String, String>,
}

/// Individual file paths for a resolved model.
///
/// Used when model files are downloaded individually (e.g. from HuggingFace
/// Hub) and may live in different cache directories rather than a single
/// self-contained model directory.
#[derive(Debug, Clone)]
pub struct ResolvedModelFiles {
    /// Path to the main model weights (`.safetensors`).
    pub model_weights: PathBuf,
    /// Path to the speech tokenizer / decoder weights.
    pub decoder_weights: PathBuf,
    /// Path to the text tokenizer file (`tokenizer.json` or directory).
    pub tokenizer: PathBuf,
    /// Path to the model config file (`config.json`), if available.
    pub config: Option<PathBuf>,
}

// ---------------------------------------------------------------------------
// SynthesisOptions — generation parameters
// ---------------------------------------------------------------------------

/// Synthesis generation parameters.
///
/// Domain-owned — mapped to `qwen3_tts::SynthesisOptions` in the infra layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisOptions {
    /// Sampling temperature.
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    /// Top-k sampling.
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    /// Nucleus sampling threshold.
    #[serde(default = "default_top_p")]
    pub top_p: f64,
    /// Repetition penalty.
    #[serde(default = "default_repetition_penalty")]
    pub repetition_penalty: f64,
    /// Random seed for reproducibility.
    pub seed: Option<u64>,
    /// Maximum generation frames (~12 frames/second of audio).
    #[serde(default = "default_max_frames")]
    pub max_frames: usize,
}

fn default_temperature() -> f64 {
    0.7
}
fn default_top_k() -> usize {
    50
}
fn default_top_p() -> f64 {
    0.9
}
fn default_repetition_penalty() -> f64 {
    1.05
}
fn default_max_frames() -> usize {
    2048
}

impl Default for SynthesisOptions {
    fn default() -> Self {
        Self {
            temperature: default_temperature(),
            top_k: default_top_k(),
            top_p: default_top_p(),
            repetition_penalty: default_repetition_penalty(),
            seed: Some(42),
            max_frames: default_max_frames(),
        }
    }
}

// ---------------------------------------------------------------------------
// SynthesisRequest — what the caller wants
// ---------------------------------------------------------------------------

/// A fully-described synthesis request. Built by merging config defaults
/// with CLI/API overrides.
#[derive(Debug, Clone)]
pub struct SynthesisRequest {
    /// Text to synthesise.
    pub text: String,
    /// Where to find the model.
    pub model_ref: ModelRef,
    /// Voice to use.
    pub voice: VoiceId,
    /// Target language.
    pub language: Language,
    /// Generation options.
    pub options: SynthesisOptions,
    /// Voice-design instruction text (VoiceDesign models only).
    pub instruct: Option<String>,
    /// Path to reference audio for voice cloning (Base models only).
    pub ref_audio_path: Option<PathBuf>,
    /// Transcript of reference audio for ICL voice cloning.
    pub ref_text: Option<String>,
    /// Pipeline stage names for pre-processing.
    pub pre_stages: Vec<String>,
    /// Pipeline stage names for post-processing.
    pub post_stages: Vec<String>,
}

// ---------------------------------------------------------------------------
// SynthesisTiming — per-phase timing breakdown
// ---------------------------------------------------------------------------

/// Timing breakdown for a synthesis operation.
#[derive(Debug, Clone, Default)]
pub struct SynthesisTiming {
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
// SynthesisResult — what comes back
// ---------------------------------------------------------------------------

/// Result of a synthesis operation.
#[derive(Debug, Clone)]
pub struct SynthesisResult {
    /// PCM f32 audio samples.
    pub audio_samples: Vec<f32>,
    /// Sample rate of the audio.
    pub sample_rate: SampleRate,
    /// Metadata bag (model variant, voice used, etc.).
    pub metadata: HashMap<String, String>,
    /// Per-phase timing breakdown.
    pub timings: SynthesisTiming,
    /// Non-fatal warnings (e.g. voice/model mismatch).
    pub warnings: Vec<String>,
}
