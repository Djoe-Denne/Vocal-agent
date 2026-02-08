//! Model resolver — routes model references to the correct provider.
//!
//! Pure routing logic with no mixed HF/local concerns.

use crate::domain::models::ResolvedModel;
use crate::domain::ports::ModelProviderPort;
use crate::domain::value_objects::ModelRef;

/// Routes a [`ModelRef`] to the appropriate [`ModelProviderPort`] based on
/// its variant (Local or HuggingFace).
///
/// This is routing only — no download or validation logic lives here.
pub struct ModelResolver {
    local_provider: Box<dyn ModelProviderPort>,
}

impl ModelResolver {
    /// Create a resolver with the local model provider.
    pub fn new(local_provider: Box<dyn ModelProviderPort>) -> Self {
        Self { local_provider }
    }

    /// Resolve a model reference to a locally-available [`ResolvedModel`].
    pub fn resolve(&self, model_ref: &ModelRef) -> anyhow::Result<ResolvedModel> {
        match model_ref {
            ModelRef::Local { .. } => self.local_provider.prepare(model_ref),
            ModelRef::HuggingFace { repo, .. } => {
                anyhow::bail!(
                    "HuggingFace model downloading is not yet supported for ASR. \
                     Please download the model manually and use --model-dir. \
                     Requested repo: {repo}"
                )
            }
        }
    }
}
