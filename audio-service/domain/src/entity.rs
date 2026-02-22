use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioTransformRequest {
    pub samples: Vec<f32>,
    pub source_sample_rate_hz: u32,
    pub target_sample_rate_hz: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformMetadata {
    pub clamped: bool,
    pub resampled: bool,
    pub input_sample_count: usize,
    pub output_sample_count: usize,
    pub source_sample_rate_hz: u32,
    pub target_sample_rate_hz: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioTransformResult {
    pub samples: Vec<f32>,
    pub sample_rate_hz: u32,
    pub metadata: TransformMetadata,
}
