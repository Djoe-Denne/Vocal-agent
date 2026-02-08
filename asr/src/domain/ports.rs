//! Domain port interfaces for ASR.
//!
//! Pure abstract contracts (traits) that infrastructure adapters must implement.

use std::path::Path;

use super::models::TranscriptionResult;

/// Abstract port for ASR transcription adapters.
///
/// Mirrors the Python `BaseTranscriber` ABC from `ptt.domain.ports`.
pub trait Transcriber: Send {
    /// Whether the underlying model is currently loaded.
    fn is_loaded(&self) -> bool;

    /// Load the model into memory.
    fn load_model(&mut self) -> anyhow::Result<()>;

    /// Unload the model from memory.
    fn unload_model(&mut self) -> anyhow::Result<()>;

    /// Transcribe an audio file at the given path.
    fn transcribe_file(&mut self, audio_path: &Path) -> anyhow::Result<TranscriptionResult>;
}
