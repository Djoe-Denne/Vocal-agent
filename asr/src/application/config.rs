//! ASR configuration.
//!
//! Configuration structs and loader, mirroring the Python
//! `ptt.application.config` pattern. Reads from a TOML file.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Top-level ASR configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct AsrConfig {
    /// Model configuration.
    #[serde(default)]
    pub model: ModelConfig,
}

/// Model-specific configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelConfig {
    /// Path to the local model weights directory.
    #[serde(default = "ModelConfig::default_model_dir")]
    pub model_dir: PathBuf,

    /// Language for transcription (e.g. "fr", "en").
    #[serde(default = "ModelConfig::default_language")]
    pub language: String,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            model_dir: Self::default_model_dir(),
            language: Self::default_language(),
        }
    }
}

impl ModelConfig {
    fn default_model_dir() -> PathBuf {
        PathBuf::from("./asr/models/Qwen3-ASR-1.7b")
    }

    fn default_language() -> String {
        "fr".to_owned()
    }
}

impl Default for AsrConfig {
    fn default() -> Self {
        Self {
            model: ModelConfig::default(),
        }
    }
}

/// Load configuration from a TOML file.
pub fn load_config(path: &Path) -> anyhow::Result<AsrConfig> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read config {}: {e}", path.display()))?;
    let config: AsrConfig = toml::from_str(&contents)?;
    Ok(config)
}
