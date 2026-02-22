use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    #[serde(default)]
    pub alignment: AlignmentConfig,
    #[serde(default)]
    pub pipeline: Option<PipelineConfig>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlignmentConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_min_word_duration_ms")]
    pub min_word_duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    #[serde(default = "default_pipeline_name")]
    pub selected: String,
    #[serde(default = "default_pipeline_definitions")]
    pub definitions: HashMap<String, PipelineDefinitionConfig>,
    #[serde(default)]
    pub plugins: PipelinePluginsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineDefinitionConfig {
    #[serde(default)]
    pub pre: Vec<PipelineStepRef>,
    #[serde(default = "default_pipeline_transcription_step")]
    pub transcription: PipelineStepRef,
    #[serde(default)]
    pub post: Vec<PipelineStepRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PipelineStepRef {
    Name(String),
    WithName { name: String },
}

impl PipelineStepRef {
    pub fn name(&self) -> &str {
        match self {
            PipelineStepRef::Name(name) => name,
            PipelineStepRef::WithName { name } => name,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PipelinePluginsConfig {
    #[serde(default)]
    pub resample: ResamplePluginConfig,
    #[serde(default)]
    pub wav2vec2: Wav2Vec2PluginConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wav2Vec2PluginConfig {
    #[serde(default = "default_wav2vec2_model_path")]
    pub model_path: String,
    #[serde(default = "default_wav2vec2_config_path")]
    pub config_path: String,
    #[serde(default = "default_wav2vec2_vocab_path")]
    pub vocab_path: String,
    #[serde(default = "default_wav2vec2_device")]
    pub device: String,
}

impl Default for Wav2Vec2PluginConfig {
    fn default() -> Self {
        Self {
            model_path: default_wav2vec2_model_path(),
            config_path: default_wav2vec2_config_path(),
            vocab_path: default_wav2vec2_vocab_path(),
            device: default_wav2vec2_device(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResamplePluginConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_sample_rate")]
    pub target_sample_rate_hz: u32,
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
            alignment: AlignmentConfig::default(),
            pipeline: None,
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

impl Default for AlignmentConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            min_word_duration_ms: default_min_word_duration_ms(),
        }
    }
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            selected: default_pipeline_name(),
            definitions: default_pipeline_definitions(),
            plugins: PipelinePluginsConfig::default(),
        }
    }
}

impl Default for PipelineDefinitionConfig {
    fn default() -> Self {
        Self {
            pre: vec![PipelineStepRef::Name("audio_clamp".to_string())],
            transcription: default_pipeline_transcription_step(),
            post: vec![PipelineStepRef::Name("wav2vec2_alignment".to_string())],
        }
    }
}

impl Default for ResamplePluginConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            target_sample_rate_hz: default_sample_rate(),
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

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
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

fn default_pipeline_name() -> String {
    "default".to_string()
}

fn default_pipeline_definitions() -> HashMap<String, PipelineDefinitionConfig> {
    let mut definitions = HashMap::new();
    definitions.insert(default_pipeline_name(), PipelineDefinitionConfig::default());
    definitions
}

fn default_pipeline_transcription_step() -> PipelineStepRef {
    PipelineStepRef::Name("whisper_transcription".to_string())
}

fn default_min_word_duration_ms() -> u64 {
    40
}

fn default_wav2vec2_model_path() -> String {
    "models/wav2vec2-fr.safetensors".to_string()
}

fn default_wav2vec2_config_path() -> String {
    "models/wav2vec2-config.json".to_string()
}

fn default_wav2vec2_vocab_path() -> String {
    "models/wav2vec2-vocab.json".to_string()
}

fn default_wav2vec2_device() -> String {
    "cpu".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_are_deterministic() {
        let cfg = AsrConfig::default();
        assert_eq!(cfg.service.audio.sample_rate_hz, 16_000);
        assert_eq!(cfg.service.asr.temperature, 0.0);
        assert!(cfg.service.alignment.enabled);
        assert_eq!(cfg.server.port, 8080);
    }
}
