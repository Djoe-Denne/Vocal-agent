//! Shared application: pipeline factory and stage registry.
//!
//! Provides a global-style registry of stage builders and functions to
//! construct [`Pipeline`] instances from TOML configuration tables.

use std::collections::HashMap;

use crate::domain::pipeline::Stage;
use crate::domain::pipeline_runner::Pipeline;

/// A builder function that constructs a [`Stage`] from a config map.
pub type StageBuilder = fn(HashMap<String, String>) -> anyhow::Result<Box<dyn Stage>>;

/// Registry of stage type names to their builder functions.
#[derive(Default)]
pub struct StageRegistry {
    builders: HashMap<String, StageBuilder>,
}

impl StageRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a stage builder under the given type name.
    pub fn register(&mut self, type_name: impl Into<String>, builder: StageBuilder) {
        self.builders.insert(type_name.into(), builder);
    }

    /// Build a single [`Pipeline`] from a pipeline definition and stages config.
    ///
    /// `pipeline_def` must contain a `"stages"` key with a comma-separated
    /// list of stage names. Each stage name is looked up in `stages_config`
    /// to retrieve its builder parameters.
    pub fn build_pipeline(
        &self,
        name: &str,
        stage_names: &[String],
        stages_config: &HashMap<String, HashMap<String, String>>,
    ) -> anyhow::Result<Pipeline> {
        let mut stages: Vec<Box<dyn Stage>> = Vec::new();

        for stage_name in stage_names {
            let stage_cfg = stages_config
                .get(stage_name.as_str())
                .cloned()
                .unwrap_or_default();

            let type_name = stage_cfg
                .get("type")
                .cloned()
                .unwrap_or_else(|| stage_name.clone());

            let builder = self.builders.get(type_name.as_str()).ok_or_else(|| {
                anyhow::anyhow!(
                    "Unknown stage type: {type_name:?}. Registered types: {:?}",
                    self.builders.keys().collect::<Vec<_>>()
                )
            })?;

            stages.push(builder(stage_cfg)?);
        }

        Ok(Pipeline::new(name, stages))
    }
}
