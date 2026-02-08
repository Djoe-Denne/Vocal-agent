//! Model resolver — routes model references to the correct provider.
//!
//! Pure routing logic with no mixed HF/local concerns.

use crate::domain::models::ResolvedModel;
use crate::domain::ports::ModelProviderPort;
use crate::domain::value_objects::ModelRef;

/// Routes a [`ModelRef`] to the appropriate [`ModelProviderPort`] based on
/// its variant (HuggingFace vs Local).
///
/// This is routing only — no download or validation logic lives here.
pub struct ModelResolver {
    hf_provider: Box<dyn ModelProviderPort>,
    local_provider: Box<dyn ModelProviderPort>,
}

impl ModelResolver {
    /// Create a resolver with the two model providers.
    pub fn new(
        hf_provider: Box<dyn ModelProviderPort>,
        local_provider: Box<dyn ModelProviderPort>,
    ) -> Self {
        Self {
            hf_provider,
            local_provider,
        }
    }

    /// Resolve a model reference to a locally-available [`ResolvedModel`].
    pub fn resolve(&self, model_ref: &ModelRef) -> anyhow::Result<ResolvedModel> {
        match model_ref {
            ModelRef::HuggingFace { .. } => self.hf_provider.prepare(model_ref),
            ModelRef::Local { .. } => self.local_provider.prepare(model_ref),
        }
    }
}
