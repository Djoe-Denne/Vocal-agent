use serde::{Deserialize, Serialize};

/// Word boundary from forced alignment.
/// Timestamps in milliseconds, u64 to match alignment-domain::WordTiming.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordBoundary {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

/// Prosody features extracted from a single audio segment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProsodyFeatures {
    /// Mean fundamental frequency over the segment, in Hz.
    /// None if the segment is unvoiced or silence.
    pub f0_mean_hz: Option<f32>,
    /// Standard deviation of F0 over the segment, in Hz.
    pub f0_std_hz: Option<f32>,
    /// RMS energy of the segment (linear scale).
    pub energy_rms: f32,
    /// Ratio of voiced frames to total frames (0.0 to 1.0).
    pub voicing_ratio: f32,
}

/// Prosody features attached to a specific word.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordFeatures {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub features: ProsodyFeatures,
}

/// Per-frame measurement (one per 10ms hop).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameMeasurement {
    pub time_ms: f64,
    pub f0_hz: Option<f32>,
    pub aperiodicity: f32,
    pub energy_rms: f32,
}

/// Full analysis output: summary + per-frame detail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentAnalysis {
    pub summary: ProsodyFeatures,
    pub frames: Vec<FrameMeasurement>,
}
