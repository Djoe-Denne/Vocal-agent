//! ASR entry point.
//!
//! Thin binary that wires infrastructure adapters to the application
//! use case and delegates to it. This is the **composition root** —
//! the only place that knows about concrete adapter types.
//!
//! Supports two modes:
//! - `asr transcribe <audio_file>` — one-shot CLI transcription
//! - `asr serve [--host] [--port]` — start the web API server

use anyhow::{Context, Result};
use clap::Parser;

use asr::application::config::ConfigService;
use asr::application::model_resolver::ModelResolver;
use asr::application::pipeline_registry::PipelineRegistry;
use asr::application::use_cases::TranscribeAudioUseCase;
use asr::infra_cli::cli::{Args, Command};
use asr::infra_aha::transcriber::AhaTranscriber;
use asr::infra_local::provider::LocalModelProvider;
use asr::infra_openclaw::post_processor::OpenClawPostProcessor;

fn main() -> Result<()> {
    let args = Args::parse();

    // -- 1. Load configuration -----------------------------------------------
    let mut config = ConfigService::load(args.config.as_deref())
        .context("Failed to load configuration")?;

    // Global CLI overrides.
    if let Some(ref device) = args.device {
        config.engine.device = device.clone();
    }
    if let Some(ref model_dir) = args.model_dir {
        config.engine.model_dir = model_dir.clone();
    }

    match args.command {
        Command::Transcribe(ref transcribe_args) => {
            run_transcribe(&args, transcribe_args, config)
        }
        #[cfg(feature = "web")]
        Command::Serve(ref serve_args) => run_serve(serve_args, config),
    }
}

fn build_use_case(
    config: asr::application::config::AsrConfig,
) -> TranscribeAudioUseCase {
    let local_provider = LocalModelProvider::new();
    let model_resolver = ModelResolver::new(Box::new(local_provider));
    let engine = AhaTranscriber::new(config.engine.clone());
    let mut pipeline_registry = PipelineRegistry::new();

    if config.openclaw.enabled {
        let openclaw_config = config.openclaw.clone();
        pipeline_registry.register_post("openclaw", move || {
            let processor = OpenClawPostProcessor::new(openclaw_config.clone())?;
            Ok(Box::new(processor))
        });
    }

    TranscribeAudioUseCase::new(
        config,
        model_resolver,
        Box::new(engine),
        pipeline_registry,
    )
}

// ---------------------------------------------------------------------------
// CLI transcription mode
// ---------------------------------------------------------------------------

fn run_transcribe(
    args: &Args,
    transcribe_args: &asr::infra_cli::cli::TranscribeArgs,
    config: asr::application::config::AsrConfig,
) -> Result<()> {
    println!("=== Qwen3 ASR ===");
    println!("Model dir : {}", config.engine.model_dir.display());
    println!("Audio     : {}", transcribe_args.audio_file.display());
    println!("Language  : {}", transcribe_args.language);
    println!("Device    : {}", config.engine.device);
    println!();

    let mut use_case = build_use_case(config);

    let language = transcribe_args
        .language_id()
        .context("Invalid language")?;
    let model_ref = args.model_ref();

    let request = use_case.build_request(
        transcribe_args.audio_file.clone(),
        model_ref,
        Some(language),
    );

    let result = use_case.execute(request)?;

    for warning in &result.warnings {
        eprintln!("Warning: {warning}");
    }

    if let Some(ref output_path) = transcribe_args.output {
        std::fs::write(output_path, &result.text)
            .with_context(|| {
                format!("Failed to write output to {}", output_path.display())
            })?;
        println!(
            "Transcript saved to {} ({:.0}ms total)",
            output_path.display(),
            result.timings.total_ms,
        );
    } else {
        println!("Transcription ({:.0}ms total):", result.timings.total_ms);
        println!("{}", result.text);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Web API server mode
// ---------------------------------------------------------------------------

#[cfg(feature = "web")]
fn run_serve(
    serve_args: &asr::infra_cli::cli::ServeArgs,
    config: asr::application::config::AsrConfig,
) -> Result<()> {
    use std::sync::Arc;
    use asr::infra_web::api;

    let bind_addr = format!("{}:{}", serve_args.host, serve_args.port);

    println!("=== Qwen3 ASR — Web API ===");
    println!("Model dir : {}", config.engine.model_dir.display());
    println!("Device    : {}", config.engine.device);
    println!("Listening : http://{bind_addr}");
    println!();

    let use_case = build_use_case(config);
    let state = Arc::new(api::AppState {
        use_case: std::sync::Mutex::new(use_case),
    });

    let app = api::router(state);

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
        println!("Server started. Endpoints:");
        println!("  GET  /health");
        println!("  POST /transcribe");
        println!();
        axum::serve(listener, app).await?;
        Ok::<(), anyhow::Error>(())
    })?;

    Ok(())
}
