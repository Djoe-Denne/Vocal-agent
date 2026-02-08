//! Domain data models for ASR.

/// Result of a transcription operation.
#[derive(Debug, Clone)]
pub struct TranscriptionResult {
    /// Transcribed text.
    pub text: String,
    /// Processing duration in seconds.
    pub duration_secs: f64,
    /// Duration of the source audio in seconds (if known).
    pub audio_duration_secs: Option<f64>,
}
