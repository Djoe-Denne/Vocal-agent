pub mod energy;
pub mod extractor;
pub mod types;
pub mod util;
pub mod yin;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FeatureError {
    #[error("audio segment too short: {actual} samples, need at least {required}")]
    SegmentTooShort { actual: usize, required: usize },

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
}

pub use energy::{rms_energy, rms_energy_frames};
pub use extractor::{FeatureExtractor, FeatureExtractorConfig};
pub use types::{FrameMeasurement, ProsodyFeatures, SegmentAnalysis, WordBoundary, WordFeatures};
pub use util::{hann_window, ms_to_samples, samples_to_ms};
pub use yin::{estimate_f0, estimate_mean_f0, F0Frame, YinConfig};
