use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordTiming {
    pub word: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub confidence: f32,
}

#[derive(Debug, Clone)]
pub struct TempoMatchRequest {
    pub tts_samples: Vec<f32>,
    pub tts_sample_rate_hz: u32,
    pub original_timings: Vec<WordTiming>,
    pub tts_timings: Vec<WordTiming>,
}

#[derive(Debug, Clone)]
pub struct TempoMatchOutput {
    pub samples: Vec<f32>,
    pub sample_rate_hz: u32,
}

#[derive(Debug, Clone)]
pub struct SegmentPlan {
    pub start_sample: usize,
    pub end_sample: usize,
    pub original_duration_samples: usize,
    pub target_duration_samples: usize,
    pub alpha: f64,
}

#[derive(Debug, Clone)]
pub struct SegmentAudio {
    pub local_samples: Vec<f32>,
    pub global_start_sample: usize,
    pub global_end_sample: usize,
    pub margin_left: usize,
    pub margin_right: usize,
    pub target_duration_samples: usize,
    pub alpha: f64,
}

#[derive(Debug, Clone)]
pub struct FrameMetrics {
    pub energy: f32,
    pub is_voiced: bool,
    pub periodicity: f32,
}

#[derive(Debug, Clone)]
pub struct SegmentFrameAnalysis {
    pub segment_index: usize,
    pub frame_length_samples: usize,
    pub hop_samples: usize,
    pub frames: Vec<FrameMetrics>,
}
