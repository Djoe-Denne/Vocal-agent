//! TTS entry point.
//!
//! Thin binary that wires infrastructure adapters to the application
//! use case and delegates to it. This is the **composition root** —
//! the only place that knows about concrete adapter types.
//!
//! Supports two modes:
//! - `tts synthesize "text"` — one-shot CLI synthesis
//! - `tts serve [--host] [--port]` — start the web API server

use anyhow::{Context, Result};
use clap::Parser;

use tts::application::config::ConfigService;
use tts::application::model_resolver::ModelResolver;
use tts::application::pipeline_registry::PipelineRegistry;
use tts::application::use_cases::SynthesizeSpeechUseCase;
use tts::domain::models::SynthesisOptions;
use tts::infra_cli::cli::{Args, Command};
use tts::infra_hf::provider::HuggingFaceModelProvider;
use tts::infra_local::provider::LocalModelProvider;
use tts::infra_qwen3::engine::Qwen3TtsEngine;

fn main() -> Result<()> {
    let args = Args::parse();

    // -- 1. Load configuration -----------------------------------------------
    let mut config = ConfigService::load(args.config.as_deref())
        .context("Failed to load configuration")?;

    // Global CLI overrides.
    if let Some(ref device) = args.device {
        config.engine.device = device.clone();
    }

    match args.command {
        Command::Synthesize(ref synth_args) => {
            run_synthesize(&args, synth_args, config)
        }
        #[cfg(feature = "web")]
        Command::Serve(ref serve_args) => run_serve(serve_args, config),
    }
}

fn build_use_case(
    config: tts::application::config::TtsConfig,
) -> SynthesizeSpeechUseCase {
    let hf_provider = HuggingFaceModelProvider::new(
        config.engine.model_cache_dir.clone(),
    );
    let local_provider = LocalModelProvider::new();

    let model_resolver = ModelResolver::new(
        Box::new(hf_provider),
        Box::new(local_provider),
    );

    let engine = Qwen3TtsEngine::new(config.engine.clone());
    let pipeline_registry = PipelineRegistry::new();

    SynthesizeSpeechUseCase::new(
        config,
        model_resolver,
        Box::new(engine),
        pipeline_registry,
    )
}

// ---------------------------------------------------------------------------
// CLI synthesis mode
// ---------------------------------------------------------------------------

fn run_synthesize(
    args: &Args,
    synth_args: &tts::infra_cli::cli::SynthesizeArgs,
    config: tts::application::config::TtsConfig,
) -> Result<()> {
    println!("=== Qwen3 TTS ===");
    println!("Text      : {:?}", synth_args.text);
    println!("Voice     : {}", synth_args.voice);
    println!("Language  : {}", synth_args.language);
    println!("Device    : {}", config.engine.device);
    println!("Output    : {}", synth_args.output.display());
    println!();

    let mut use_case = build_use_case(config.clone());

    let voice = synth_args.voice_id().context("Invalid voice")?;
    let language = synth_args.language_id().context("Invalid language")?;
    let model_ref = args.model_ref();

    let options = SynthesisOptions {
        temperature: synth_args
            .temperature
            .unwrap_or(config.defaults.temperature),
        top_k: synth_args.top_k.unwrap_or(config.defaults.top_k),
        top_p: synth_args.top_p.unwrap_or(config.defaults.top_p),
        repetition_penalty: synth_args
            .repetition_penalty
            .unwrap_or(config.defaults.repetition_penalty),
        seed: synth_args.seed.or(config.defaults.seed),
        max_frames: config.defaults.max_frames,
    };

    let request = use_case.build_request(
        synth_args.text.clone(),
        model_ref,
        Some(voice),
        Some(language),
        Some(options),
        synth_args.instruct.clone(),
        synth_args.ref_audio.clone(),
        synth_args.ref_text.clone(),
    );

    let result = use_case.execute(request)?;

    for warning in &result.warnings {
        eprintln!("Warning: {warning}");
    }

    let audio = qwen3_tts::AudioBuffer::new(
        result.audio_samples,
        result.sample_rate.0,
    );
    audio.save(&synth_args.output)?;

    println!(
        "Audio saved to {} ({:.1}s, {:.0}ms total)",
        synth_args.output.display(),
        audio.samples.len() as f64 / audio.sample_rate as f64,
        result.timings.total_ms,
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Web API server mode
// ---------------------------------------------------------------------------

#[cfg(feature = "web")]
fn run_serve(
    serve_args: &tts::infra_cli::cli::ServeArgs,
    config: tts::application::config::TtsConfig,
) -> Result<()> {
    use std::sync::Arc;
    use tts::infra_web::api;

    let bind_addr = format!("{}:{}", serve_args.host, serve_args.port);

    println!("=== Qwen3 TTS — Web API ===");
    println!("Device    : {}", config.engine.device);
    println!("Listening : http://{bind_addr}");
    println!();

    let voices_dir = config.engine.voices_dir.clone();
    let use_case = build_use_case(config);
    let state = Arc::new(api::AppState {
        use_case: std::sync::Mutex::new(use_case),
        voices_dir,
    });

    let app = api::router(state);

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
        println!("Server started. Endpoints:");
        println!("  GET  /health");
        println!("  POST /v1/audio/speech");
        println!("  GET  /v1/audio/voices");
        println!();
        axum::serve(listener, app).await?;
        Ok::<(), anyhow::Error>(())
    })?;

    Ok(())
}
