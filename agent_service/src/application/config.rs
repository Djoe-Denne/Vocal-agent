use std::path::Path;
use std::time::Duration;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AgentServiceConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub asr: AsrClientConfig,
    #[serde(default)]
    pub openclaw: OpenClawClientConfig,
}

impl Default for AgentServiceConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            asr: AsrClientConfig::default(),
            openclaw: OpenClawClientConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_server_host")]
    pub host: String,
    #[serde(default = "default_server_port")]
    pub port: u16,
}

fn default_server_host() -> String {
    "127.0.0.1".to_owned()
}

fn default_server_port() -> u16 {
    3010
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_server_host(),
            port: default_server_port(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AsrClientConfig {
    #[serde(default = "default_asr_base_url")]
    pub base_url: String,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_asr_base_url() -> String {
    "http://127.0.0.1:3001".to_owned()
}

fn default_timeout_ms() -> u64 {
    30_000
}

impl AsrClientConfig {
    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }
}

impl Default for AsrClientConfig {
    fn default() -> Self {
        Self {
            base_url: default_asr_base_url(),
            timeout_ms: default_timeout_ms(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenClawClientConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_openclaw_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_enabled() -> bool {
    true
}

fn default_openclaw_base_url() -> String {
    "http://127.0.0.1:18789".to_owned()
}

impl OpenClawClientConfig {
    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }
}

impl Default for OpenClawClientConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            base_url: default_openclaw_base_url(),
            model: None,
            token: None,
            timeout_ms: default_timeout_ms(),
        }
    }
}

pub struct ConfigService;

impl ConfigService {
    pub fn load_from_file(path: &Path) -> anyhow::Result<AgentServiceConfig> {
        let contents = std::fs::read_to_string(path).map_err(|e| {
            anyhow::anyhow!("Failed to read config {}: {e}", path.display())
        })?;
        let config: AgentServiceConfig = toml::from_str(&contents)?;
        Ok(config)
    }

    pub fn load_or_default(path: Option<&Path>) -> anyhow::Result<AgentServiceConfig> {
        match path {
            Some(p) => Self::load_from_file(p),
            None => Ok(AgentServiceConfig::default()),
        }
    }

    pub fn apply_env_overrides(mut config: AgentServiceConfig) -> AgentServiceConfig {
        if let Ok(host) = std::env::var("AGENT_SERVICE_HOST") {
            config.server.host = host;
        }
        if let Ok(port) = std::env::var("AGENT_SERVICE_PORT") {
            if let Ok(port) = port.parse::<u16>() {
                config.server.port = port;
            }
        }
        if let Ok(base_url) = std::env::var("ASR_BASE_URL") {
            config.asr.base_url = base_url;
        }
        if let Ok(base_url) = std::env::var("OPENCLAW_BASE_URL") {
            config.openclaw.base_url = base_url;
        }
        if let Ok(model) = std::env::var("OPENCLAW_MODEL") {
            config.openclaw.model = Some(model);
        }
        if let Ok(token) = std::env::var("OPENCLAW_TOKEN") {
            config.openclaw.token = Some(token);
        }
        config
    }

    pub fn load(path: Option<&Path>) -> anyhow::Result<AgentServiceConfig> {
        let config = Self::load_or_default(path)?;
        Ok(Self::apply_env_overrides(config))
    }
}
