use tempo_domain::TempoPipelineStage;

use crate::engine::TempoPipelineEngine;
use crate::stages::{
    AudioPrepareStage, F0EstimationStage, FrameAnalysisStage, PitchMarkStage,
    SegmentExtractionStage, SegmentPlanStage, StretchRegionStage, VoicedZoneStage,
};

pub struct TempoPipelineBuilder {
    stages: Vec<Box<dyn TempoPipelineStage>>,
}

impl TempoPipelineBuilder {
    pub fn new() -> Self {
        Self {
            stages: Vec::new(),
        }
    }

    pub fn push(mut self, stage: Box<dyn TempoPipelineStage>) -> Self {
        self.stages.push(stage);
        self
    }

    pub fn build(self) -> TempoPipelineEngine {
        TempoPipelineEngine::new(self.stages)
    }

    /// Build the full pipeline with all implemented stages in spec order.
    pub fn default_pipeline() -> TempoPipelineEngine {
        Self::new()
            // Phase 1 -- socle
            .push(Box::new(AudioPrepareStage))
            .push(Box::new(SegmentPlanStage))
            .push(Box::new(SegmentExtractionStage))
            .push(Box::new(FrameAnalysisStage::default()))
            // Phase 2 -- analyse
            .push(Box::new(F0EstimationStage))
            .push(Box::new(VoicedZoneStage))
            .push(Box::new(PitchMarkStage))
            .push(Box::new(StretchRegionStage))
            .build()
    }
}

impl Default for TempoPipelineBuilder {
    fn default() -> Self {
        Self::new()
    }
}
