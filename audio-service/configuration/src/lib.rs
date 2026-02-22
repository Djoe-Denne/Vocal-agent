use serde::{Deserialize, Serialize};

use rustycog_config::{
    load_config_fresh, ConfigError, ConfigLoader, HasLoggingConfig, HasServerConfig,
    LoggingConfig, ServerConfig,
};

pub use rustycog_logger::setup_logging;

pub type AppConfig = AudioConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub transformations: TransformationsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformationsConfig {
    #[serde(default = "default_sample_rate")]
    pub sample_rate_hz: u32,
    #[serde(default = "default_chunk_ms")]
    pub chunk_ms: u32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            logging: LoggingConfig::default(),
            transformations: TransformationsConfig::default(),
        }
    }
}

impl Default for TransformationsConfig {
    fn default() -> Self {
        Self {
            sample_rate_hz: default_sample_rate(),
            chunk_ms: default_chunk_ms(),
        }
    }
}

impl ConfigLoader<AudioConfig> for AudioConfig {
    fn create_default() -> AudioConfig {
        AudioConfig::default()
    }

    fn config_prefix() -> &'static str {
        "AUDIO_SERVICE"
    }
}

impl HasServerConfig for AudioConfig {
    fn server_config(&self) -> &ServerConfig {
        &self.server
    }

    fn set_server_config(&mut self, config: ServerConfig) {
        self.server = config;
    }
}

impl HasLoggingConfig for AudioConfig {
    fn logging_config(&self) -> &LoggingConfig {
        &self.logging
    }

    fn set_logging_config(&mut self, config: LoggingConfig) {
        self.logging = config;
    }
}

pub fn load_config() -> Result<AudioConfig, ConfigError> {
    load_config_fresh::<AudioConfig>()
}

fn default_sample_rate() -> u32 {
    16_000
}

fn default_chunk_ms() -> u32 {
    500
}

