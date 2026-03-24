use crate::entity::{SegmentAudio, SegmentFrameAnalysis, SegmentPlan, WordTiming};
use crate::DomainError;

pub struct TempoPipelineContext {
    pub samples: Vec<f32>,
    pub sample_rate_hz: u32,
    pub original_timings: Vec<WordTiming>,
    pub tts_timings: Vec<WordTiming>,
    pub segment_plans: Vec<SegmentPlan>,
    pub segment_audios: Vec<SegmentAudio>,
    pub frame_analyses: Vec<SegmentFrameAnalysis>,
}

impl TempoPipelineContext {
    pub fn new(
        samples: Vec<f32>,
        sample_rate_hz: u32,
        original_timings: Vec<WordTiming>,
        tts_timings: Vec<WordTiming>,
    ) -> Self {
        Self {
            samples,
            sample_rate_hz,
            original_timings,
            tts_timings,
            segment_plans: Vec::new(),
            segment_audios: Vec::new(),
            frame_analyses: Vec::new(),
        }
    }
}

pub trait TempoPipelineStage: Send + Sync {
    fn name(&self) -> &'static str;
    fn execute(&self, context: &mut TempoPipelineContext) -> Result<(), DomainError>;
}
