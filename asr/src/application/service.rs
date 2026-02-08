//! ASR application service.
//!
//! Provides the [`AsrService`] orchestrator that takes a trait-object
//! [`Transcriber`] and delegates to it, keeping infrastructure behind
//! the hexagonal boundary.

use std::path::Path;

use crate::domain::models::TranscriptionResult;
use crate::domain::ports::Transcriber;

/// Application-level ASR service.
///
/// Owns a concrete transcriber behind a trait object and orchestrates
/// the transcription workflow.
pub struct AsrService {
    transcriber: Box<dyn Transcriber>,
}

impl AsrService {
    pub fn new(transcriber: Box<dyn Transcriber>) -> Self {
        Self { transcriber }
    }

    /// Ensure the model is loaded, then transcribe an audio file.
    pub fn transcribe_file(&mut self, audio_path: &Path) -> anyhow::Result<TranscriptionResult> {
        if !self.transcriber.is_loaded() {
            self.transcriber.load_model()?;
        }
        self.transcriber.transcribe_file(audio_path)
    }

    /// Load the underlying model.
    pub fn load_model(&mut self) -> anyhow::Result<()> {
        self.transcriber.load_model()
    }

    /// Unload the underlying model.
    pub fn unload_model(&mut self) -> anyhow::Result<()> {
        self.transcriber.unload_model()
    }

    /// Whether the model is currently loaded.
    pub fn is_loaded(&self) -> bool {
        self.transcriber.is_loaded()
    }
}
