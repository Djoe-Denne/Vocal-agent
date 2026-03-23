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
    #[serde(default = "default_wsola_window_ms")]
    pub wsola_window_ms: u32,
    #[serde(default = "default_wsola_overlap_ratio")]
    pub wsola_overlap_ratio: f32,
    #[serde(default = "default_crossfade_ms")]
    pub crossfade_ms: u32,
    #[serde(default = "default_stretch_tolerance")]
    pub stretch_tolerance: f32,
    #[serde(default = "default_max_stretch_ratio")]
    pub max_stretch_ratio: f32,
    #[serde(default = "default_min_stretch_ratio")]
    pub min_stretch_ratio: f32,
    #[serde(default = "default_silence_threshold_db")]
    pub silence_threshold_db: f32,
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
            wsola_window_ms: default_wsola_window_ms(),
            wsola_overlap_ratio: default_wsola_overlap_ratio(),
            crossfade_ms: default_crossfade_ms(),
            stretch_tolerance: default_stretch_tolerance(),
            max_stretch_ratio: default_max_stretch_ratio(),
            min_stretch_ratio: default_min_stretch_ratio(),
            silence_threshold_db: default_silence_threshold_db(),
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

fn default_wsola_window_ms() -> u32 {
    30
}

fn default_wsola_overlap_ratio() -> f32 {
    0.75
}

fn default_crossfade_ms() -> u32 {
    8
}

fn default_stretch_tolerance() -> f32 {
    0.05
}

fn default_max_stretch_ratio() -> f32 {
    6.0
}

fn default_min_stretch_ratio() -> f32 {
    0.15
}

fn default_silence_threshold_db() -> f32 {
    -40.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_are_deterministic() {
        let cfg = TempoConfig::default();
        assert_eq!(cfg.tempo.sample_rate_hz, 16_000);
        assert_eq!(cfg.tempo.wsola_window_ms, 30);
        assert!((cfg.tempo.wsola_overlap_ratio - 0.75).abs() < f32::EPSILON);
        assert_eq!(cfg.server.port, 8080);
    }
}
