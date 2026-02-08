//! CLI adapter for ASR.
//!
//! Clap-based argument parsing with subcommands for transcription (one-shot)
//! and serving (web API).

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::domain::value_objects::{Language, ModelRef};

/// Qwen3 ASR — Automatic Speech Recognition.
#[derive(Parser, Debug)]
#[command(
    name = "asr",
    about = "Automatic Speech Recognition with Qwen3-ASR"
)]
pub struct Args {
    /// Path to a TOML config file.
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    /// Path to the model weights directory.
    #[arg(long, global = true)]
    pub model_dir: Option<PathBuf>,

    /// Compute device: auto, cpu, cuda, cuda:N, metal.
    #[arg(long, global = true)]
    pub device: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Transcribe an audio file (one-shot CLI mode).
    Transcribe(TranscribeArgs),

    /// Start the ASR web API server.
    #[cfg(feature = "web")]
    Serve(ServeArgs),
}

/// Arguments for the `transcribe` subcommand.
#[derive(Parser, Debug)]
pub struct TranscribeArgs {
    /// Audio file to transcribe.
    pub audio_file: PathBuf,

    /// Language hint (e.g. "fr", "en", "chinese").
    #[arg(long, default_value = "fr")]
    pub language: String,

    /// Output file for the transcript (prints to stdout if omitted).
    #[arg(long, short)]
    pub output: Option<PathBuf>,
}

impl TranscribeArgs {
    /// Parse the language string into a domain `Language`.
    pub fn language_id(&self) -> anyhow::Result<Language> {
        self.language.parse()
    }
}

/// Arguments for the `serve` subcommand.
#[cfg(feature = "web")]
#[derive(Parser, Debug)]
pub struct ServeArgs {
    /// Host address to bind to.
    #[arg(long, default_value = "0.0.0.0")]
    pub host: String,

    /// Port to listen on.
    #[arg(long, default_value_t = 3000)]
    pub port: u16,
}

impl Args {
    /// Build a `ModelRef` from the global `--model-dir` flag.
    pub fn model_ref(&self) -> Option<ModelRef> {
        self.model_dir.as_ref().map(|dir| ModelRef::Local {
            path: dir.clone(),
        })
    }
}
