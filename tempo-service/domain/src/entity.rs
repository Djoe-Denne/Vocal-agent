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

// --- Phase 2 entities ---

#[derive(Debug, Clone)]
pub struct PitchFrame {
    pub center_sample: usize,
    pub voiced: bool,
    pub f0_hz: f32,
    pub period_samples: f32,
}

#[derive(Debug, Clone)]
pub struct SegmentPitchData {
    pub segment_index: usize,
    pub frames: Vec<PitchFrame>,
}

#[derive(Debug, Clone)]
pub struct VoicedRegion {
    pub start_sample: usize,
    pub end_sample: usize,
    pub mean_f0: f32,
    pub mean_period_samples: f32,
    pub stability_score: f32,
}

#[derive(Debug, Clone)]
pub struct SegmentVoicedRegions {
    pub segment_index: usize,
    pub regions: Vec<VoicedRegion>,
}

#[derive(Debug, Clone)]
pub struct PitchMark {
    pub sample_index: usize,
    pub local_period_samples: f32,
    pub confidence: f32,
}

#[derive(Debug, Clone)]
pub struct SegmentPitchMarks {
    pub segment_index: usize,
    pub marks: Vec<PitchMark>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StretchMode {
    Pause,
    VoicedPsola,
    KeepNearConstant,
}

#[derive(Debug, Clone)]
pub struct StretchRegion {
    pub start_sample: usize,
    pub end_sample: usize,
    pub local_alpha: f64,
    pub mode: StretchMode,
}

#[derive(Debug, Clone)]
pub struct SegmentStretchPlan {
    pub segment_index: usize,
    pub regions: Vec<StretchRegion>,
}

// --- Phase 3 entities ---

#[derive(Debug, Clone)]
pub struct Grain {
    pub analysis_mark_index: usize,
    pub center_sample: usize,
    pub windowed_samples: Vec<f32>,
}

#[derive(Debug, Clone)]
pub struct SegmentGrains {
    pub segment_index: usize,
    pub grains: Vec<Grain>,
}

#[derive(Debug, Clone)]
pub struct SynthesisMark {
    pub output_sample_index: usize,
    pub mapped_analysis_mark_index: usize,
}

#[derive(Debug, Clone)]
pub struct SegmentSynthesisGrid {
    pub segment_index: usize,
    pub marks: Vec<SynthesisMark>,
}

#[derive(Debug, Clone)]
pub struct SynthesisPlacement {
    pub output_center_sample: usize,
    pub source_grain_index: usize,
}

#[derive(Debug, Clone)]
pub struct SegmentSynthesisPlan {
    pub segment_index: usize,
    pub placements: Vec<SynthesisPlacement>,
}
