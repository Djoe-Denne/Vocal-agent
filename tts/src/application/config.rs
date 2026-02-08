//! TTS configuration.
//!
//! TOML-driven configuration with serde. Supports named model presets,
//! pipeline stage lists, and engine settings. Override priority:
//!
//!   Request (CLI/API) > Environment variables > TOML file > Hardcoded defaults

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::domain::value_objects::{Language, ModelRef, VoiceId};

// ---------------------------------------------------------------------------
// Top-level config
// ---------------------------------------------------------------------------

/// Top-level TTS configuration (deserialised from TOML).
#[derive(Debug, Clone, Deserialize)]
pub struct TtsConfig {
    /// Default synthesis settings.
    #[serde(default)]
    pub defaults: DefaultsConfig,

    /// Engine / runtime settings.
    #[serde(default)]
    pub engine: EngineConfig,

    /// Pipeline stage lists.
    #[serde(default)]
    pub pipeline: PipelineConfig,

    /// Named model presets (e.g. `[models.fast_local]`).
    #[serde(default)]
    pub models: HashMap<String, ModelRef>,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            defaults: DefaultsConfig::default(),
            engine: EngineConfig::default(),
            pipeline: PipelineConfig::default(),
            models: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// DefaultsConfig
// ---------------------------------------------------------------------------

/// Default synthesis parameters applied when not overridden by the request.
#[derive(Debug, Clone, Deserialize)]
pub struct DefaultsConfig {
    /// Default voice.
    #[serde(default)]
    pub voice: VoiceId,

    /// Default language.
    #[serde(default)]
    pub language: Language,

    /// Default model source.
    #[serde(default)]
    pub model: ModelRef,

    /// Default sampling temperature.
    #[serde(default = "default_temperature")]
    pub temperature: f64,

    /// Default top-k.
    #[serde(default = "default_top_k")]
    pub top_k: usize,

    /// Default top-p.
    #[serde(default = "default_top_p")]
    pub top_p: f64,

    /// Default repetition penalty.
    #[serde(default = "default_repetition_penalty")]
    pub repetition_penalty: f64,

    /// Default random seed.
    #[serde(default = "default_seed")]
    pub seed: Option<u64>,

    /// Default max generation frames.
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
fn default_seed() -> Option<u64> {
    Some(42)
}
fn default_max_frames() -> usize {
    2048
}

impl Default for DefaultsConfig {
    fn default() -> Self {
        Self {
            voice: VoiceId::default(),
            language: Language::default(),
            model: ModelRef::default(),
            temperature: default_temperature(),
            top_k: default_top_k(),
            top_p: default_top_p(),
            repetition_penalty: default_repetition_penalty(),
            seed: default_seed(),
            max_frames: default_max_frames(),
        }
    }
}

// ---------------------------------------------------------------------------
// EngineConfig
// ---------------------------------------------------------------------------

/// Engine / runtime configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct EngineConfig {
    /// Compute device: `"auto"`, `"cpu"`, `"cuda"`, `"cuda:0"`, `"metal"`.
    #[serde(default = "default_device")]
    pub device: String,

    /// Data type: `"f32"`, `"bf16"`.
    #[serde(default = "default_dtype")]
    pub dtype: String,

    /// Whether to enable flash-attention (CUDA only).
    #[serde(default)]
    pub flash_attn: bool,

    /// Local cache directory for downloaded models.
    #[serde(default = "default_model_cache_dir")]
    pub model_cache_dir: PathBuf,

    /// Directory containing voice clone profiles.
    ///
    /// Each subdirectory is a voice name containing `reference.wav`
    /// and optionally `transcript.txt`. For example:
    ///
    /// ```text
    /// voices/
    ///   justamon/
    ///     reference.wav
    ///     transcript.txt   (optional — enables ICL mode for better quality)
    /// ```
    #[serde(default = "default_voices_dir")]
    pub voices_dir: PathBuf,
}

fn default_device() -> String {
    "auto".to_owned()
}
fn default_dtype() -> String {
    "bf16".to_owned()
}
fn default_model_cache_dir() -> PathBuf {
    PathBuf::from("./models")
}
fn default_voices_dir() -> PathBuf {
    PathBuf::from("./voices")
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            device: default_device(),
            dtype: default_dtype(),
            flash_attn: false,
            model_cache_dir: default_model_cache_dir(),
            voices_dir: default_voices_dir(),
        }
    }
}

// ---------------------------------------------------------------------------
// PipelineConfig
// ---------------------------------------------------------------------------

/// Pipeline stage configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct PipelineConfig {
    /// Ordered list of pre-processor stage names.
    #[serde(default)]
    pub pre: Vec<String>,

    /// Ordered list of post-processor stage names.
    #[serde(default)]
    pub post: Vec<String>,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            pre: Vec::new(),
            post: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// ConfigService
// ---------------------------------------------------------------------------

/// Loads and merges configuration from TOML files and environment variables.
pub struct ConfigService;

impl ConfigService {
    /// Load configuration from a TOML file.
    pub fn load_from_file(path: &Path) -> anyhow::Result<TtsConfig> {
        let contents = std::fs::read_to_string(path).map_err(|e| {
            anyhow::anyhow!("Failed to read config {}: {e}", path.display())
        })?;
        let config: TtsConfig = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Load configuration from file if it exists, otherwise return defaults.
    pub fn load_or_default(path: Option<&Path>) -> anyhow::Result<TtsConfig> {
        match path {
            Some(p) => Self::load_from_file(p),
            None => Ok(TtsConfig::default()),
        }
    }

    /// Apply environment variable overrides to config.
    ///
    /// Supported env vars:
    /// - `TTS_DEVICE` — overrides `engine.device`
    /// - `TTS_MODEL_CACHE_DIR` — overrides `engine.model_cache_dir`
    pub fn apply_env_overrides(mut config: TtsConfig) -> TtsConfig {
        if let Ok(device) = std::env::var("TTS_DEVICE") {
            config.engine.device = device;
        }
        if let Ok(cache_dir) = std::env::var("TTS_MODEL_CACHE_DIR") {
            config.engine.model_cache_dir = PathBuf::from(cache_dir);
        }
        config
    }

    /// Full config loading pipeline: file → env overrides.
    pub fn load(path: Option<&Path>) -> anyhow::Result<TtsConfig> {
        let config = Self::load_or_default(path)?;
        Ok(Self::apply_env_overrides(config))
    }
}
