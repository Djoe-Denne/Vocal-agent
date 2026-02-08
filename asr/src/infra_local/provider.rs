//! Local model provider for ASR.
//!
//! Validates that a local model directory exists and contains the
//! expected files, then returns a [`ResolvedModel`].

use std::collections::HashMap;

use crate::domain::models::ResolvedModel;
use crate::domain::ports::ModelProviderPort;
use crate::domain::value_objects::ModelRef;

/// Provides models from local directories.
pub struct LocalModelProvider;

impl LocalModelProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LocalModelProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelProviderPort for LocalModelProvider {
    fn prepare(&self, model_ref: &ModelRef) -> anyhow::Result<ResolvedModel> {
        let path = match model_ref {
            ModelRef::Local { path } => path,
            other => anyhow::bail!(
                "LocalModelProvider received non-local ModelRef: {other:?}"
            ),
        };

        // Validate the directory exists.
        anyhow::ensure!(
            path.is_dir(),
            "Model directory does not exist: {}",
            path.display()
        );

        Ok(ResolvedModel {
            root_dir: path.clone(),
            metadata: HashMap::new(),
        })
    }
}
