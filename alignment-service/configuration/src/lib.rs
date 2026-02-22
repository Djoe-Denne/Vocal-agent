use serde::{Deserialize, Serialize};

use rustycog_config::{
    load_config_fresh, ConfigError, ConfigLoader, HasLoggingConfig, HasServerConfig,
    LoggingConfig, ServerConfig,
};

pub use rustycog_logger::setup_logging;

pub type AppConfig = AlignmentConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlignmentConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub alignment: AlignmentRuntimeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlignmentRuntimeConfig {
    #[serde(default = "default_sample_rate")]
    pub sample_rate_hz: u32,
    #[serde(default = "default_model_path")]
    pub model_path: String,
    #[serde(default = "default_config_path")]
    pub config_path: String,
    #[serde(default = "default_vocab_path")]
    pub vocab_path: String,
    #[serde(default = "default_device")]
    pub device: String,
}

impl Default for AlignmentConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            logging: LoggingConfig::default(),
            alignment: AlignmentRuntimeConfig::default(),
        }
    }
}

impl Default for AlignmentRuntimeConfig {
    fn default() -> Self {
        Self {
            sample_rate_hz: default_sample_rate(),
            model_path: default_model_path(),
            config_path: default_config_path(),
            vocab_path: default_vocab_path(),
            device: default_device(),
        }
    }
}

impl ConfigLoader<AlignmentConfig> for AlignmentConfig {
    fn create_default() -> AlignmentConfig {
        AlignmentConfig::default()
    }

    fn config_prefix() -> &'static str {
        "ALIGNMENT_SERVICE"
    }
}

impl HasServerConfig for AlignmentConfig {
    fn server_config(&self) -> &ServerConfig {
        &self.server
    }

    fn set_server_config(&mut self, config: ServerConfig) {
        self.server = config;
    }
}

impl HasLoggingConfig for AlignmentConfig {
    fn logging_config(&self) -> &LoggingConfig {
        &self.logging
    }

    fn set_logging_config(&mut self, config: LoggingConfig) {
        self.logging = config;
    }
}

pub fn load_config() -> Result<AlignmentConfig, ConfigError> {
    load_config_fresh::<AlignmentConfig>()
}

fn default_sample_rate() -> u32 {
    16_000
}

fn default_model_path() -> String {
    "models/wav2vec2-fr.safetensors".to_string()
}

fn default_config_path() -> String {
    "models/wav2vec2-config.json".to_string()
}

fn default_vocab_path() -> String {
    "models/wav2vec2-vocab.json".to_string()
}

fn default_device() -> String {
    "cpu".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_are_deterministic() {
        let cfg = AlignmentConfig::default();
        assert_eq!(cfg.alignment.sample_rate_hz, 16_000);
        assert_eq!(cfg.alignment.device, "cpu");
        assert_eq!(cfg.server.port, 8080);
    }
}
