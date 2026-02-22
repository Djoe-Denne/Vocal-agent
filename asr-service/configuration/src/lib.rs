use serde::{Deserialize, Serialize};

use rustycog_config::{
    load_config_fresh, ConfigError, ConfigLoader, HasLoggingConfig, HasQueueConfig,
    HasServerConfig, LoggingConfig, QueueConfig, ServerConfig,
};

pub use rustycog_logger::setup_logging;

pub type AppConfig = AsrConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub queue: QueueConfig,
    #[serde(default)]
    pub service: ServiceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub asr: AsrRuntimeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    #[serde(default = "default_sample_rate")]
    pub sample_rate_hz: u32,
    #[serde(default = "default_chunk_ms")]
    pub chunk_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrRuntimeConfig {
    #[serde(default = "default_model_path")]
    pub model_path: String,
    #[serde(default = "default_language")]
    pub default_language: String,
    #[serde(default = "default_supported_languages")]
    pub supported_languages: Vec<String>,
    #[serde(default)]
    pub temperature: f32,
    #[serde(default = "default_threads")]
    pub threads: usize,
    #[serde(default = "default_dtw_preset")]
    pub dtw_preset: String,
    #[serde(default = "default_dtw_mem_size")]
    pub dtw_mem_size: usize,
}

impl Default for AsrConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            logging: LoggingConfig::default(),
            queue: QueueConfig::default(),
            service: ServiceConfig::default(),
        }
    }
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            audio: AudioConfig::default(),
            asr: AsrRuntimeConfig::default(),
        }
    }
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate_hz: default_sample_rate(),
            chunk_ms: default_chunk_ms(),
        }
    }
}

impl Default for AsrRuntimeConfig {
    fn default() -> Self {
        Self {
            model_path: default_model_path(),
            default_language: default_language(),
            supported_languages: default_supported_languages(),
            temperature: 0.0,
            threads: default_threads(),
            dtw_preset: default_dtw_preset(),
            dtw_mem_size: default_dtw_mem_size(),
        }
    }
}

impl ConfigLoader<AsrConfig> for AsrConfig {
    fn create_default() -> AsrConfig {
        AsrConfig::default()
    }

    fn config_prefix() -> &'static str {
        "ASR_SERVICE"
    }
}

impl HasServerConfig for AsrConfig {
    fn server_config(&self) -> &ServerConfig {
        &self.server
    }

    fn set_server_config(&mut self, config: ServerConfig) {
        self.server = config;
    }
}

impl HasLoggingConfig for AsrConfig {
    fn logging_config(&self) -> &LoggingConfig {
        &self.logging
    }

    fn set_logging_config(&mut self, config: LoggingConfig) {
        self.logging = config;
    }
}

impl HasQueueConfig for AsrConfig {
    fn queue_config(&self) -> &QueueConfig {
        &self.queue
    }

    fn set_queue_config(&mut self, config: QueueConfig) {
        self.queue = config;
    }
}

pub fn load_config() -> Result<AsrConfig, ConfigError> {
    load_config_fresh::<AsrConfig>()
}

fn default_sample_rate() -> u32 {
    16_000
}

fn default_chunk_ms() -> u32 {
    500
}

fn default_model_path() -> String {
    "models/ggml-base.bin".to_string()
}

fn default_language() -> String {
    "auto".to_string()
}

fn default_supported_languages() -> Vec<String> {
    vec!["fr".to_string(), "en".to_string()]
}

fn default_threads() -> usize {
    4
}

fn default_dtw_preset() -> String {
    "base".to_string()
}

fn default_dtw_mem_size() -> usize {
    128
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_are_deterministic() {
        let cfg = AsrConfig::default();
        assert_eq!(cfg.service.audio.sample_rate_hz, 16_000);
        assert_eq!(cfg.service.asr.temperature, 0.0);
        assert_eq!(cfg.server.port, 8080);
    }
}
