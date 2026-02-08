//! Domain port interfaces for TTS.
//!
//! Pure abstract contracts (traits) that infrastructure adapters must
//! implement. The domain never depends on concrete implementations.

use super::models::{ResolvedModel, SynthesisRequest, SynthesisResult};
use super::pipeline::{PostProcessorContext, PreProcessorContext};
use super::value_objects::ModelRef;

// ---------------------------------------------------------------------------
// TtsEnginePort — synthesis engine
// ---------------------------------------------------------------------------

/// Abstract port for TTS synthesis engines.
///
/// The engine receives a resolved (local) model and a fully-built request.
/// It never knows whether the model came from HuggingFace or a local path.
pub trait TtsEnginePort: Send {
    /// Synthesise speech from the given request using the resolved model.
    fn synthesize(
        &mut self,
        model: &ResolvedModel,
        request: &SynthesisRequest,
    ) -> anyhow::Result<SynthesisResult>;
}

// ---------------------------------------------------------------------------
// ModelProviderPort — model resolution / download
// ---------------------------------------------------------------------------

/// Abstract port for model providers.
///
/// Each provider handles one kind of `ModelRef` (HuggingFace or local).
/// The provider prepares the model so it is locally available and returns
/// a `ResolvedModel` with local paths.
pub trait ModelProviderPort: Send {
    /// Prepare a model from the given reference.
    ///
    /// For HuggingFace: download and cache.
    /// For local: validate directory and read config.
    fn prepare(&self, model_ref: &ModelRef) -> anyhow::Result<ResolvedModel>;
}

// ---------------------------------------------------------------------------
// PreProcessor / PostProcessor — pipeline stages
// ---------------------------------------------------------------------------

/// A pre-processing stage that runs before TTS inference.
///
/// Pre-processors can normalise text, detect language, chunk long inputs, etc.
pub trait PreProcessor: Send {
    /// Unique identifier used in pipeline configuration.
    fn name(&self) -> &str;

    /// Transform the pre-processor context in-place.
    fn process(&self, ctx: PreProcessorContext) -> anyhow::Result<PreProcessorContext>;
}

/// A post-processing stage that runs after TTS inference.
///
/// Post-processors can trim silence, normalise loudness, resample, encode, etc.
pub trait PostProcessor: Send {
    /// Unique identifier used in pipeline configuration.
    fn name(&self) -> &str;

    /// Transform the post-processor context in-place.
    fn process(&self, ctx: PostProcessorContext) -> anyhow::Result<PostProcessorContext>;
}
