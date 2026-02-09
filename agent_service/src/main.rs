use std::sync::Arc;

use agent_service::application::config::{AgentServiceConfig, ConfigService};
use agent_service::application::use_cases::ProcessAudioUseCase;
use agent_service::domain::ports::ConversationalAgentPort;
use agent_service::infra_asr_http::client::AsrHttpClient;
use agent_service::infra_openclaw_http::client::OpenClawHttpClient;
use agent_service::infra_web::api;
use anyhow::{Context, Result};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "agent_service")]
#[command(about = "ASR -> conversational agent orchestrator service")]
struct Args {
    /// Optional TOML config path.
    #[arg(long)]
    config: Option<std::path::PathBuf>,

    /// Override bind host.
    #[arg(long)]
    host: Option<String>,

    /// Override bind port.
    #[arg(long)]
    port: Option<u16>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let mut config = ConfigService::load(args.config.as_deref())
        .context("Failed to load configuration")?;

    if let Some(host) = args.host {
        config.server.host = host;
    }
    if let Some(port) = args.port {
        config.server.port = port;
    }

    run_serve(config)
}

fn build_use_case(config: &AgentServiceConfig) -> anyhow::Result<ProcessAudioUseCase> {
    let asr_client = AsrHttpClient::from_config(&config.asr)?;

    let conversational_agent: Option<Box<dyn ConversationalAgentPort>> =
        if config.openclaw.enabled {
            Some(Box::new(OpenClawHttpClient::from_config(&config.openclaw)?))
        } else {
            None
        };

    Ok(ProcessAudioUseCase::new(Box::new(asr_client), conversational_agent))
}

fn run_serve(config: AgentServiceConfig) -> Result<()> {
    let bind_addr = format!("{}:{}", config.server.host, config.server.port);

    println!("=== Agent Service — Web API ===");
    println!("ASR base URL     : {}", config.asr.base_url);
    println!("OpenClaw enabled : {}", config.openclaw.enabled);
    println!("Listening        : http://{bind_addr}");
    println!();

    let use_case = build_use_case(&config)?;
    let state = Arc::new(api::AppState {
        use_case: std::sync::Mutex::new(use_case),
    });
    let app = api::router(state);

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
        println!("Server started. Endpoints:");
        println!("  GET  /health");
        println!("  POST /process");
        println!();
        axum::serve(listener, app).await?;
        Ok::<(), anyhow::Error>(())
    })?;

    Ok(())
}
