use tempo_domain::TempoPipelineStage;

use crate::engine::TempoPipelineEngine;
use crate::stages::{AudioPrepareStage, FrameAnalysisStage, SegmentExtractionStage, SegmentPlanStage};

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

    /// Build the canonical Phase 1 (socle) pipeline with all stages in spec order.
    pub fn default_pipeline() -> TempoPipelineEngine {
        Self::new()
            .push(Box::new(AudioPrepareStage))
            .push(Box::new(SegmentPlanStage))
            .push(Box::new(SegmentExtractionStage))
            .push(Box::new(FrameAnalysisStage::default()))
            .build()
    }
}

impl Default for TempoPipelineBuilder {
    fn default() -> Self {
        Self::new()
    }
}
