//! Pipeline registry — builds pre/post processor chains from configuration.
//!
//! Similar to TTS's `PipelineRegistry` but typed for the ASR-specific
//! `PreProcessor` / `PostProcessor` traits.

use std::collections::HashMap;

use crate::domain::ports::{PostProcessor, PreProcessor};

/// Builder function type for pre-processors.
pub type PreProcessorBuilder =
    Box<dyn Fn() -> anyhow::Result<Box<dyn PreProcessor>> + Send + Sync>;

/// Builder function type for post-processors.
pub type PostProcessorBuilder =
    Box<dyn Fn() -> anyhow::Result<Box<dyn PostProcessor>> + Send + Sync>;

/// Registry of named pre/post processor builders.
///
/// Processors are registered by name, then chains are built from
/// ordered name lists in the pipeline configuration.
pub struct PipelineRegistry {
    pre_builders: HashMap<String, PreProcessorBuilder>,
    post_builders: HashMap<String, PostProcessorBuilder>,
}

impl PipelineRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            pre_builders: HashMap::new(),
            post_builders: HashMap::new(),
        }
    }

    /// Register a pre-processor builder under the given name.
    pub fn register_pre(
        &mut self,
        name: impl Into<String>,
        builder: impl Fn() -> anyhow::Result<Box<dyn PreProcessor>> + Send + Sync + 'static,
    ) {
        self.pre_builders.insert(name.into(), Box::new(builder));
    }

    /// Register a post-processor builder under the given name.
    pub fn register_post(
        &mut self,
        name: impl Into<String>,
        builder: impl Fn() -> anyhow::Result<Box<dyn PostProcessor>> + Send + Sync + 'static,
    ) {
        self.post_builders.insert(name.into(), Box::new(builder));
    }

    /// Build an ordered chain of pre-processors from stage names.
    pub fn build_pre_chain(
        &self,
        stage_names: &[String],
    ) -> anyhow::Result<Vec<Box<dyn PreProcessor>>> {
        let mut chain = Vec::with_capacity(stage_names.len());
        for name in stage_names {
            let builder = self.pre_builders.get(name.as_str()).ok_or_else(|| {
                anyhow::anyhow!(
                    "Unknown pre-processor: {name:?}. Registered: {:?}",
                    self.pre_builders.keys().collect::<Vec<_>>()
                )
            })?;
            chain.push(builder()?);
        }
        Ok(chain)
    }

    /// Build an ordered chain of post-processors from stage names.
    pub fn build_post_chain(
        &self,
        stage_names: &[String],
    ) -> anyhow::Result<Vec<Box<dyn PostProcessor>>> {
        let mut chain = Vec::with_capacity(stage_names.len());
        for name in stage_names {
            let builder = self.post_builders.get(name.as_str()).ok_or_else(|| {
                anyhow::anyhow!(
                    "Unknown post-processor: {name:?}. Registered: {:?}",
                    self.post_builders.keys().collect::<Vec<_>>()
                )
            })?;
            chain.push(builder()?);
        }
        Ok(chain)
    }
}

impl Default for PipelineRegistry {
    fn default() -> Self {
        Self::new()
    }
}
