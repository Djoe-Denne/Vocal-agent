use crate::entity::{
    SegmentAudio, SegmentFrameAnalysis, SegmentPitchData, SegmentPitchMarks, SegmentPlan,
    SegmentStretchPlan, SegmentVoicedRegions, WordTiming,
};
use crate::DomainError;

pub struct TempoPipelineContext {
    pub samples: Vec<f32>,
    pub sample_rate_hz: u32,
    pub original_timings: Vec<WordTiming>,
    pub tts_timings: Vec<WordTiming>,
    pub segment_plans: Vec<SegmentPlan>,
    pub segment_audios: Vec<SegmentAudio>,
    pub frame_analyses: Vec<SegmentFrameAnalysis>,
    pub pitch_data: Vec<SegmentPitchData>,
    pub voiced_regions: Vec<SegmentVoicedRegions>,
    pub pitch_marks: Vec<SegmentPitchMarks>,
    pub stretch_plans: Vec<SegmentStretchPlan>,
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
            pitch_data: Vec::new(),
            voiced_regions: Vec::new(),
            pitch_marks: Vec::new(),
            stretch_plans: Vec::new(),
        }
    }
}

pub trait TempoPipelineStage: Send + Sync {
    fn name(&self) -> &'static str;
    fn execute(&self, context: &mut TempoPipelineContext) -> Result<(), DomainError>;
}
