//! ASR configuration.
//!
//! TOML-driven configuration with serde. Supports pipeline stage lists
//! and engine settings. Override priority:
//!
//!   Request (CLI/API) > Environment variables > TOML file > Hardcoded defaults

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::domain::value_objects::Language;

// ---------------------------------------------------------------------------
// Top-level config
// ---------------------------------------------------------------------------

/// Top-level ASR configuration (deserialised from TOML).
#[derive(Debug, Clone, Deserialize)]
pub struct AsrConfig {
    /// Default transcription settings.
    #[serde(default)]
    pub defaults: DefaultsConfig,

    /// Engine / runtime settings.
    #[serde(default)]
    pub engine: EngineConfig,

    /// Pipeline stage lists.
    #[serde(default)]
    pub pipeline: PipelineConfig,

    /// OpenClaw agent delivery settings.
    #[serde(default)]
    pub openclaw: OpenClawConfig,
}

impl Default for AsrConfig {
    fn default() -> Self {
        Self {
            defaults: DefaultsConfig::default(),
            engine: EngineConfig::default(),
            pipeline: PipelineConfig::default(),
            openclaw: OpenClawConfig::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// DefaultsConfig
// ---------------------------------------------------------------------------

/// Default transcription parameters applied when not overridden by the request.
#[derive(Debug, Clone, Deserialize)]
pub struct DefaultsConfig {
    /// Default language hint.
    #[serde(default)]
    pub language: Language,
}

impl Default for DefaultsConfig {
    fn default() -> Self {
        Self {
            language: Language::default(),
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

    /// Local model weights directory.
    #[serde(default = "default_model_dir")]
    pub model_dir: PathBuf,
}

fn default_device() -> String {
    "auto".to_owned()
}

fn default_model_dir() -> PathBuf {
    PathBuf::from("./models/Qwen3-ASR-1.7b")
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            device: default_device(),
            model_dir: default_model_dir(),
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
// OpenClawConfig
// ---------------------------------------------------------------------------

/// OpenClaw delivery settings (HTTP-based).
#[derive(Debug, Clone, Deserialize)]
pub struct OpenClawConfig {
    /// Enable delivery to OpenClaw.
    #[serde(default)]
    pub enabled: bool,

    /// OpenClaw gateway base URL.
    #[serde(default = "default_openclaw_base_url")]
    pub base_url: String,

    /// OpenClaw model id (provider/model-name).
    #[serde(default)]
    pub model: Option<String>,

    /// OpenClaw gateway token (from env).
    #[serde(default)]
    pub token: Option<String>,
}

fn default_openclaw_base_url() -> String {
    "http://127.0.0.1:18789".to_owned()
}

impl Default for OpenClawConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_openclaw_base_url(),
            model: None,
            token: None,
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
    pub fn load_from_file(path: &Path) -> anyhow::Result<AsrConfig> {
        let contents = std::fs::read_to_string(path).map_err(|e| {
            anyhow::anyhow!("Failed to read config {}: {e}", path.display())
        })?;
        let config: AsrConfig = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Load configuration from file if it exists, otherwise return defaults.
    pub fn load_or_default(path: Option<&Path>) -> anyhow::Result<AsrConfig> {
        match path {
            Some(p) => Self::load_from_file(p),
            None => Ok(AsrConfig::default()),
        }
    }

    /// Apply environment variable overrides to config.
    ///
    /// Supported env vars:
    /// - `ASR_DEVICE` — overrides `engine.device`
    /// - `ASR_MODEL_DIR` — overrides `engine.model_dir`
    pub fn apply_env_overrides(mut config: AsrConfig) -> AsrConfig {
        if let Ok(device) = std::env::var("ASR_DEVICE") {
            config.engine.device = device;
        }
        if let Ok(model_dir) = std::env::var("ASR_MODEL_DIR") {
            config.engine.model_dir = PathBuf::from(model_dir);
        }
        if let Ok(token) = std::env::var("OPENCLAW_TOKEN") {
            config.openclaw.token = Some(token);
        }
        config
    }

    /// Full config loading pipeline: file -> env overrides.
    pub fn load(path: Option<&Path>) -> anyhow::Result<AsrConfig> {
        let config = Self::load_or_default(path)?;
        Ok(Self::apply_env_overrides(config))
    }
}
