//! Domain value objects for TTS.
//!
//! Pure Rust types with no external framework dependencies.
//! These are the building blocks for domain models and port contracts.

use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ModelRef — where to find a model (HuggingFace or local directory)
// ---------------------------------------------------------------------------

/// Reference to a model source. Serde-tagged for TOML config.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ModelRef {
    /// Model hosted on HuggingFace Hub.
    #[serde(rename = "huggingface")]
    HuggingFace {
        /// Repository ID, e.g. `"Qwen/Qwen3-TTS-12Hz-1.7B-Base"`.
        repo: String,
        /// Git revision (branch, tag, or commit). Defaults to `"main"`.
        #[serde(default = "default_revision")]
        revision: String,
    },
    /// Model stored in a local directory.
    Local {
        /// Absolute or relative path to the model directory.
        path: PathBuf,
    },
}

fn default_revision() -> String {
    "main".to_owned()
}

impl Default for ModelRef {
    fn default() -> Self {
        Self::HuggingFace {
            repo: "Qwen/Qwen3-TTS-12Hz-1.7B-Base".to_owned(),
            revision: default_revision(),
        }
    }
}

// ---------------------------------------------------------------------------
// ModelId — logical name for a model preset
// ---------------------------------------------------------------------------

/// Logical name that maps to a model preset in configuration
/// (e.g. `"fast_local"`, `"hq_cloud"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelId(pub String);

impl fmt::Display for ModelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for ModelId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

// ---------------------------------------------------------------------------
// VoiceId — which voice to use
// ---------------------------------------------------------------------------

/// Identifies the voice for synthesis.
///
/// Preset variants match the 9 built-in speakers in CustomVoice models.
/// `Custom` is used for voice-clone references or voice-design descriptions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceId {
    // Preset speakers (CustomVoice models)
    Serena,
    Vivian,
    UncleFu,
    Ryan,
    Aiden,
    OnoAnna,
    Sohee,
    Eric,
    Dylan,
    /// Custom voice (clone reference path or voice-design description).
    Custom(String),
}

impl Default for VoiceId {
    fn default() -> Self {
        Self::Ryan
    }
}

impl fmt::Display for VoiceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Custom(s) => write!(f, "custom({s})"),
            other => write!(f, "{other:?}"),
        }
    }
}

impl FromStr for VoiceId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "serena" => Ok(Self::Serena),
            "vivian" => Ok(Self::Vivian),
            "unclefu" | "uncle_fu" => Ok(Self::UncleFu),
            "ryan" => Ok(Self::Ryan),
            "aiden" => Ok(Self::Aiden),
            "onoanna" | "ono_anna" => Ok(Self::OnoAnna),
            "sohee" => Ok(Self::Sohee),
            "eric" => Ok(Self::Eric),
            "dylan" => Ok(Self::Dylan),
            other => Ok(Self::Custom(other.to_owned())),
        }
    }
}

// ---------------------------------------------------------------------------
// Language
// ---------------------------------------------------------------------------

/// Target language for synthesis.
///
/// Domain-owned enum — mapped to `qwen3_tts::Language` in the infra layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    English,
    Chinese,
    Japanese,
    Korean,
    German,
    French,
    Russian,
    Portuguese,
    Spanish,
    Italian,
}

impl Default for Language {
    fn default() -> Self {
        Self::English
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::English => "English",
            Self::Chinese => "Chinese",
            Self::Japanese => "Japanese",
            Self::Korean => "Korean",
            Self::German => "German",
            Self::French => "French",
            Self::Russian => "Russian",
            Self::Portuguese => "Portuguese",
            Self::Spanish => "Spanish",
            Self::Italian => "Italian",
        };
        f.write_str(s)
    }
}

impl FromStr for Language {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "english" | "en" => Ok(Self::English),
            "chinese" | "zh" => Ok(Self::Chinese),
            "japanese" | "ja" => Ok(Self::Japanese),
            "korean" | "ko" => Ok(Self::Korean),
            "german" | "de" => Ok(Self::German),
            "french" | "fr" => Ok(Self::French),
            "russian" | "ru" => Ok(Self::Russian),
            "portuguese" | "pt" => Ok(Self::Portuguese),
            "spanish" | "es" => Ok(Self::Spanish),
            "italian" | "it" => Ok(Self::Italian),
            other => anyhow::bail!("Unknown language: {other:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// AudioFormat
// ---------------------------------------------------------------------------

/// Output audio encoding format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormat {
    /// Standard WAV (PCM16).
    Wav,
    /// Raw PCM f32 samples.
    Raw,
}

impl Default for AudioFormat {
    fn default() -> Self {
        Self::Wav
    }
}

// ---------------------------------------------------------------------------
// SampleRate
// ---------------------------------------------------------------------------

/// Audio sample rate in Hz. Qwen3-TTS outputs 24 kHz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SampleRate(pub u32);

impl Default for SampleRate {
    fn default() -> Self {
        Self(24_000)
    }
}

impl fmt::Display for SampleRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} Hz", self.0)
    }
}
