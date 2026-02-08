//! CLI adapter for TTS.
//!
//! Clap-based argument parsing with subcommands for synthesis (one-shot)
//! and serving (web API).

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::domain::value_objects::{Language, ModelRef, VoiceId};

/// Qwen3 TTS — Text-to-Speech synthesis.
#[derive(Parser, Debug)]
#[command(
    name = "tts",
    about = "Text-to-Speech synthesis with Qwen3-TTS"
)]
pub struct Args {
    /// Path to a TOML config file.
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    /// Local model directory path.
    #[arg(long, global = true)]
    pub model_dir: Option<PathBuf>,

    /// HuggingFace model ID (e.g. "Qwen/Qwen3-TTS-12Hz-1.7B-Base").
    #[arg(long, global = true)]
    pub model_id: Option<String>,

    /// Compute device: auto, cpu, cuda, cuda:N, metal.
    #[arg(long, global = true)]
    pub device: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Synthesise speech from text (one-shot CLI mode).
    Synthesize(SynthesizeArgs),

    /// Start the TTS web API server.
    #[cfg(feature = "web")]
    Serve(ServeArgs),
}

/// Arguments for the `synthesize` subcommand.
#[derive(Parser, Debug)]
pub struct SynthesizeArgs {
    /// Text to synthesise.
    pub text: String,

    /// Voice to use (preset name or custom identifier).
    #[arg(long, default_value = "ryan")]
    pub voice: String,

    /// Target language.
    #[arg(long, default_value = "english")]
    pub language: String,

    /// Output WAV file path.
    #[arg(long, short, default_value = "output.wav")]
    pub output: PathBuf,

    /// Random seed for reproducibility.
    #[arg(long)]
    pub seed: Option<u64>,

    /// Sampling temperature.
    #[arg(long)]
    pub temperature: Option<f64>,

    /// Top-k sampling.
    #[arg(long)]
    pub top_k: Option<usize>,

    /// Top-p nucleus sampling threshold.
    #[arg(long)]
    pub top_p: Option<f64>,

    /// Repetition penalty.
    #[arg(long)]
    pub repetition_penalty: Option<f64>,

    /// Voice design instruction text (VoiceDesign models only).
    #[arg(long)]
    pub instruct: Option<String>,

    /// Reference audio WAV for voice cloning (Base models only).
    #[arg(long)]
    pub ref_audio: Option<PathBuf>,

    /// Reference transcript for ICL voice cloning (requires --ref-audio).
    #[arg(long)]
    pub ref_text: Option<String>,
}

impl SynthesizeArgs {
    /// Parse the voice string into a domain `VoiceId`.
    pub fn voice_id(&self) -> anyhow::Result<VoiceId> {
        self.voice.parse()
    }

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
    /// Build a `ModelRef` from the global CLI arguments.
    ///
    /// Priority: `--model-dir` (local) > `--model-id` (HuggingFace) > None.
    pub fn model_ref(&self) -> Option<ModelRef> {
        if let Some(dir) = &self.model_dir {
            Some(ModelRef::Local { path: dir.clone() })
        } else if let Some(id) = &self.model_id {
            Some(ModelRef::HuggingFace {
                repo: id.clone(),
                revision: "main".to_owned(),
            })
        } else {
            None
        }
    }
}
