pub mod builder;
pub mod engine;
pub mod stages;

use async_trait::async_trait;
use tempo_domain::{
    DomainError, TempoMatchOutput, TempoMatchPort, TempoMatchRequest, TempoPipelineContext,
};

use crate::builder::TempoPipelineBuilder;
use crate::engine::TempoPipelineEngine;

pub struct TempoMatchAdapter {
    engine: TempoPipelineEngine,
}

impl TempoMatchAdapter {
    pub fn new() -> Self {
        Self {
            engine: TempoPipelineBuilder::default_pipeline(),
        }
    }

    pub fn with_engine(engine: TempoPipelineEngine) -> Self {
        Self { engine }
    }
}

impl Default for TempoMatchAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TempoMatchPort for TempoMatchAdapter {
    async fn match_tempo(
        &self,
        request: TempoMatchRequest,
    ) -> Result<TempoMatchOutput, DomainError> {
        let mut context = TempoPipelineContext::new(
            request.tts_samples,
            request.tts_sample_rate_hz,
            request.original_timings,
            request.tts_timings,
        );

        self.engine.run(&mut context)?;

        Ok(TempoMatchOutput {
            samples: context.samples,
            sample_rate_hz: context.sample_rate_hz,
        })
    }
}
