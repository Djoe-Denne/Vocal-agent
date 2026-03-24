use serde::{Deserialize, Serialize};

use rustycog_config::{
    load_config_fresh, ConfigError, ConfigLoader, HasLoggingConfig, HasServerConfig,
    LoggingConfig, ServerConfig,
};

pub use rustycog_logger::setup_logging;

pub type AppConfig = TempoConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempoConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub tempo: TempoRuntimeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempoRuntimeConfig {
    #[serde(default = "default_sample_rate")]
    pub sample_rate_hz: u32,
}

impl Default for TempoConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            logging: LoggingConfig::default(),
            tempo: TempoRuntimeConfig::default(),
        }
    }
}

impl Default for TempoRuntimeConfig {
    fn default() -> Self {
        Self {
            sample_rate_hz: default_sample_rate(),
        }
    }
}

impl ConfigLoader<TempoConfig> for TempoConfig {
    fn create_default() -> TempoConfig {
        TempoConfig::default()
    }

    fn config_prefix() -> &'static str {
        "TEMPO_SERVICE"
    }
}

impl HasServerConfig for TempoConfig {
    fn server_config(&self) -> &ServerConfig {
        &self.server
    }

    fn set_server_config(&mut self, config: ServerConfig) {
        self.server = config;
    }
}

impl HasLoggingConfig for TempoConfig {
    fn logging_config(&self) -> &LoggingConfig {
        &self.logging
    }

    fn set_logging_config(&mut self, config: LoggingConfig) {
        self.logging = config;
    }
}

pub fn load_config() -> Result<TempoConfig, ConfigError> {
    load_config_fresh::<TempoConfig>()
}

fn default_sample_rate() -> u32 {
    16_000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_are_deterministic() {
        let cfg = TempoConfig::default();
        assert_eq!(cfg.tempo.sample_rate_hz, 16_000);
        assert_eq!(cfg.server.port, 8080);
    }
}
