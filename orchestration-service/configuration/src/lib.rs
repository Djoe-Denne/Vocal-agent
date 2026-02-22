use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use rustycog_config::{
    load_config_fresh, ConfigError, ConfigLoader, HasLoggingConfig, HasQueueConfig,
    HasServerConfig, LoggingConfig, QueueConfig, ServerConfig,
};

pub use rustycog_logger::setup_logging;

pub type AppConfig = OrchestrationConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationConfig {
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
    #[serde(default = "default_audio_endpoint")]
    pub audio: GrpcEndpointConfig,
    #[serde(default = "default_asr_endpoint")]
    pub asr: GrpcEndpointConfig,
    #[serde(default = "default_alignment_endpoint")]
    pub alignment: GrpcEndpointConfig,
    #[serde(default)]
    pub pipeline: PipelineConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcEndpointConfig {
    #[serde(default = "default_grpc_host")]
    pub host: String,
    #[serde(default = "default_grpc_port")]
    pub port: u16,
    #[serde(default)]
    pub tls_enabled: bool,
    #[serde(default = "default_grpc_connect_timeout_ms")]
    pub connect_timeout_ms: u64,
    #[serde(default = "default_grpc_request_timeout_ms")]
    pub request_timeout_ms: u64,
    #[serde(default = "default_grpc_max_message_bytes")]
    pub max_decoding_message_bytes: usize,
    #[serde(default = "default_grpc_max_message_bytes")]
    pub max_encoding_message_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    #[serde(default = "default_pipeline_name")]
    pub selected: String,
    #[serde(default = "default_pipeline_definitions")]
    pub definitions: HashMap<String, PipelineDefinitionConfig>,
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

impl Default for OrchestrationConfig {
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
            audio: default_audio_endpoint(),
            asr: default_asr_endpoint(),
            alignment: default_alignment_endpoint(),
            pipeline: PipelineConfig::default(),
        }
    }
}

impl Default for GrpcEndpointConfig {
    fn default() -> Self {
        Self {
            host: default_grpc_host(),
            port: default_grpc_port(),
            tls_enabled: false,
            connect_timeout_ms: default_grpc_connect_timeout_ms(),
            request_timeout_ms: default_grpc_request_timeout_ms(),
            max_decoding_message_bytes: default_grpc_max_message_bytes(),
            max_encoding_message_bytes: default_grpc_max_message_bytes(),
        }
    }
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            selected: default_pipeline_name(),
            definitions: default_pipeline_definitions(),
        }
    }
}

impl Default for PipelineDefinitionConfig {
    fn default() -> Self {
        Self {
            pre: vec![PipelineStepRef::Name("audio_transform".to_string())],
            transcription: default_pipeline_transcription_step(),
            post: vec![PipelineStepRef::Name("alignment_enrich".to_string())],
        }
    }
}

impl ConfigLoader<OrchestrationConfig> for OrchestrationConfig {
    fn create_default() -> OrchestrationConfig {
        OrchestrationConfig::default()
    }

    fn config_prefix() -> &'static str {
        "ORCHESTRATION_SERVICE"
    }
}

impl HasServerConfig for OrchestrationConfig {
    fn server_config(&self) -> &ServerConfig {
        &self.server
    }

    fn set_server_config(&mut self, config: ServerConfig) {
        self.server = config;
    }
}

impl HasLoggingConfig for OrchestrationConfig {
    fn logging_config(&self) -> &LoggingConfig {
        &self.logging
    }

    fn set_logging_config(&mut self, config: LoggingConfig) {
        self.logging = config;
    }
}

impl HasQueueConfig for OrchestrationConfig {
    fn queue_config(&self) -> &QueueConfig {
        &self.queue
    }

    fn set_queue_config(&mut self, config: QueueConfig) {
        self.queue = config;
    }
}

pub fn load_config() -> Result<OrchestrationConfig, ConfigError> {
    load_config_fresh::<OrchestrationConfig>()
}

fn default_grpc_host() -> String {
    "127.0.0.1".to_string()
}

fn default_grpc_port() -> u16 {
    8080
}

fn default_grpc_connect_timeout_ms() -> u64 {
    3_000
}

fn default_grpc_request_timeout_ms() -> u64 {
    60_000
}

fn default_grpc_max_message_bytes() -> usize {
    64 * 1024 * 1024
}

fn default_audio_endpoint() -> GrpcEndpointConfig {
    GrpcEndpointConfig {
        port: 8081,
        ..GrpcEndpointConfig::default()
    }
}

fn default_asr_endpoint() -> GrpcEndpointConfig {
    GrpcEndpointConfig {
        port: 8080,
        ..GrpcEndpointConfig::default()
    }
}

fn default_alignment_endpoint() -> GrpcEndpointConfig {
    GrpcEndpointConfig {
        port: 8082,
        ..GrpcEndpointConfig::default()
    }
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
    PipelineStepRef::Name("asr_transcribe".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_are_deterministic() {
        let cfg = OrchestrationConfig::default();
        assert_eq!(cfg.service.audio.port, 8081);
        assert_eq!(cfg.service.asr.port, 8080);
        assert_eq!(cfg.service.alignment.port, 8082);
        assert_eq!(cfg.server.port, 8080);
    }
}
