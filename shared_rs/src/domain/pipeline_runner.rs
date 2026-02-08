//! Shared domain: pipeline runner.
//!
//! A [`Pipeline`] is a named, ordered sequence of [`Stage`] instances.

use std::time::Instant;

use super::pipeline::{PipelineContext, Stage, StageResult};

/// Named, ordered sequence of stages.
pub struct Pipeline {
    pub name: String,
    stages: Vec<Box<dyn Stage>>,
}

impl Pipeline {
    pub fn new(name: impl Into<String>, stages: Vec<Box<dyn Stage>>) -> Self {
        Self {
            name: name.into(),
            stages,
        }
    }

    /// Execute every stage in order, recording timing in `ctx.stage_results`.
    pub fn run(&self, mut ctx: PipelineContext) -> anyhow::Result<PipelineContext> {
        for stage in &self.stages {
            let t0 = Instant::now();
            ctx = stage.process(ctx)?;
            ctx.stage_results.push(StageResult {
                stage: stage.name().to_owned(),
                elapsed_secs: t0.elapsed().as_secs_f64(),
            });
        }
        Ok(ctx)
    }

    /// Load heavy resources for every stage.
    pub fn load_all(&mut self) -> anyhow::Result<()> {
        for stage in &mut self.stages {
            stage.load()?;
        }
        Ok(())
    }

    /// Free resources for every stage.
    pub fn unload_all(&mut self) -> anyhow::Result<()> {
        for stage in &mut self.stages {
            stage.unload()?;
        }
        Ok(())
    }
}
