//! Domain value objects for ASR.
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
    /// Model hosted on HuggingFace Hub (future use).
    #[serde(rename = "huggingface")]
    HuggingFace {
        /// Repository ID, e.g. `"Qwen/Qwen3-ASR-1.7b"`.
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
        Self::Local {
            path: PathBuf::from("./models/Qwen3-ASR-1.7b"),
        }
    }
}

// ---------------------------------------------------------------------------
// Language
// ---------------------------------------------------------------------------

/// Target language for transcription.
///
/// Domain-owned enum used as a language hint for the ASR engine.
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
        Self::French
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

/// ISO 639-1 code for a [`Language`].
impl Language {
    pub fn code(&self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Chinese => "zh",
            Self::Japanese => "ja",
            Self::Korean => "ko",
            Self::German => "de",
            Self::French => "fr",
            Self::Russian => "ru",
            Self::Portuguese => "pt",
            Self::Spanish => "es",
            Self::Italian => "it",
        }
    }
}

// ---------------------------------------------------------------------------
// SampleRate
// ---------------------------------------------------------------------------

/// Audio sample rate in Hz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SampleRate(pub u32);

impl Default for SampleRate {
    fn default() -> Self {
        Self(16_000)
    }
}

impl fmt::Display for SampleRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} Hz", self.0)
    }
}
