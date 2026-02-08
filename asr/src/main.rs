//! ASR CLI entry point.
//!
//! Thin binary that wires the infrastructure adapter to the application
//! service and delegates to it. Mirrors the Python `ptt/__main__.py` +
//! `ptt/infra_cli/cli.py` pattern.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

use asr::application::config::{load_config, AsrConfig, ModelConfig};
use asr::application::service::AsrService;
use asr::infra_aha::transcriber::AhaTranscriber;

/// Qwen3 ASR transcription CLI.
#[derive(Parser)]
#[command(name = "asr", about = "Automatic Speech Recognition with Qwen3-ASR")]
struct Args {
    /// Audio file to transcribe.
    audio_file: PathBuf,

    /// Path to the model weights directory.
    #[arg(long)]
    model_dir: Option<PathBuf>,

    /// Path to a TOML config file.
    #[arg(long)]
    config: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Load configuration (file -> CLI overrides -> defaults).
    let mut config = match &args.config {
        Some(path) => load_config(path).context("Failed to load config")?,
        None => AsrConfig::default(),
    };

    // CLI flags override config file values.
    if let Some(model_dir) = args.model_dir {
        config.model = ModelConfig {
            model_dir,
            ..config.model
        };
    }

    println!("=== Qwen3 ASR ===");
    println!("Model dir : {}", config.model.model_dir.display());
    println!("Audio     : {}", args.audio_file.display());
    println!();

    // Wire infrastructure adapter -> application service.
    let adapter = AhaTranscriber::new(config.model.clone());
    let mut service = AsrService::new(Box::new(adapter));

    // Run transcription.
    let result = service.transcribe_file(&args.audio_file)?;

    println!("Transcription ({:.1}s):", result.duration_secs);
    println!("{}", result.text);

    Ok(())
}
