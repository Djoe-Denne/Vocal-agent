//! TTS CLI entry point.
//!
//! Thin binary that wires infrastructure adapters to the application
//! use case and delegates to it. This is the **composition root** —
//! the only place that knows about concrete adapter types.

use anyhow::{Context, Result};
use clap::Parser;

use tts::application::config::ConfigService;
use tts::application::model_resolver::ModelResolver;
use tts::application::pipeline_registry::PipelineRegistry;
use tts::application::use_cases::SynthesizeSpeechUseCase;
use tts::domain::models::SynthesisOptions;
use tts::infra_cli::cli::CliArgs;
use tts::infra_hf::provider::HuggingFaceModelProvider;
use tts::infra_local::provider::LocalModelProvider;
use tts::infra_qwen3::engine::Qwen3TtsEngine;

fn main() -> Result<()> {
    let args = CliArgs::parse();

    // ── 1. Load configuration ─────────────────────────────────────
    let mut config = ConfigService::load(args.config.as_deref())
        .context("Failed to load configuration")?;

    // CLI device override.
    if let Some(ref device) = args.device {
        config.engine.device = device.clone();
    }

    println!("=== Qwen3 TTS ===");
    println!("Text      : {:?}", args.text);
    println!("Voice     : {}", args.voice);
    println!("Language  : {}", args.language);
    println!("Device    : {}", config.engine.device);
    println!("Output    : {}", args.output.display());
    println!();

    // ── 2. Construct providers ────────────────────────────────────
    let hf_provider = HuggingFaceModelProvider::new(
        config.engine.model_cache_dir.clone(),
    );
    let local_provider = LocalModelProvider::new();

    let model_resolver = ModelResolver::new(
        Box::new(hf_provider),
        Box::new(local_provider),
    );

    // ── 3. Construct engine ───────────────────────────────────────
    let engine = Qwen3TtsEngine::new(config.engine.clone());

    // ── 4. Construct pipeline registry (empty for now) ────────────
    let pipeline_registry = PipelineRegistry::new();

    // ── 5. Construct use case ─────────────────────────────────────
    let mut use_case = SynthesizeSpeechUseCase::new(
        config.clone(),
        model_resolver,
        Box::new(engine),
        pipeline_registry,
    );

    // ── 6. Build request from CLI args ────────────────────────────
    let voice = args.voice_id().context("Invalid voice")?;
    let language = args.language_id().context("Invalid language")?;
    let model_ref = args.model_ref();

    // Build options with CLI overrides.
    let options = SynthesisOptions {
        temperature: args.temperature.unwrap_or(config.defaults.temperature),
        top_k: args.top_k.unwrap_or(config.defaults.top_k),
        top_p: args.top_p.unwrap_or(config.defaults.top_p),
        repetition_penalty: args
            .repetition_penalty
            .unwrap_or(config.defaults.repetition_penalty),
        seed: args.seed.or(config.defaults.seed),
        max_frames: config.defaults.max_frames,
    };

    let request = use_case.build_request(
        args.text,
        model_ref,
        Some(voice),
        Some(language),
        Some(options),
        args.instruct,
        args.ref_audio,
        args.ref_text,
    );

    // ── 7. Run synthesis ──────────────────────────────────────────
    let result = use_case.execute(request)?;

    // ── 8. Print warnings ─────────────────────────────────────────
    for warning in &result.warnings {
        eprintln!("Warning: {warning}");
    }

    // ── 9. Write output ───────────────────────────────────────────
    let audio = qwen3_tts::AudioBuffer::new(
        result.audio_samples,
        result.sample_rate.0,
    );
    audio.save(&args.output)?;

    println!(
        "Audio saved to {} ({:.1}s, {:.0}ms total)",
        args.output.display(),
        audio.samples.len() as f64 / audio.sample_rate as f64,
        result.timings.total_ms,
    );

    Ok(())
}
