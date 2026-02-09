//! Domain port interfaces for ASR.
//!
//! Pure abstract contracts (traits) that infrastructure adapters must
//! implement. The domain never depends on concrete implementations.

use super::models::{ResolvedModel, TranscriptionRequest, TranscriptionResult};
use super::pipeline::{PostProcessorContext, PreProcessorContext};
use super::value_objects::ModelRef;

// ---------------------------------------------------------------------------
// AsrEnginePort — transcription engine
// ---------------------------------------------------------------------------

/// Abstract port for ASR transcription engines.
///
/// The engine receives a resolved (local) model and a fully-built request.
/// It never knows whether the model came from HuggingFace or a local path.
pub trait AsrEnginePort: Send {
    /// Transcribe audio from the given request using the resolved model.
    fn transcribe(
        &mut self,
        model: &ResolvedModel,
        request: &TranscriptionRequest,
    ) -> anyhow::Result<TranscriptionResult>;
}

// ---------------------------------------------------------------------------
// ModelProviderPort — model resolution / validation
// ---------------------------------------------------------------------------

/// Abstract port for model providers.
///
/// Each provider handles one kind of `ModelRef` (local or HuggingFace).
/// The provider prepares the model so it is locally available and returns
/// a `ResolvedModel` with local paths.
pub trait ModelProviderPort: Send {
    /// Prepare a model from the given reference.
    ///
    /// For local: validate directory and return paths.
    /// For HuggingFace: download and cache (future).
    fn prepare(&self, model_ref: &ModelRef) -> anyhow::Result<ResolvedModel>;
}

// ---------------------------------------------------------------------------
// PreProcessor / PostProcessor — pipeline stages
// ---------------------------------------------------------------------------

/// A pre-processing stage that runs before ASR inference.
///
/// Pre-processors can resample audio, trim silence, run VAD, etc.
pub trait PreProcessor: Send {
    /// Unique identifier used in pipeline configuration.
    fn name(&self) -> &str;

    /// Transform the pre-processor context in-place.
    fn process(&self, ctx: PreProcessorContext) -> anyhow::Result<PreProcessorContext>;
}

/// A post-processing stage that runs after ASR inference.
///
/// Post-processors can normalise text, add punctuation, filter profanity, etc.
pub trait PostProcessor: Send {
    /// Unique identifier used in pipeline configuration.
    fn name(&self) -> &str;

    /// Transform the post-processor context in-place.
    fn process(&self, ctx: PostProcessorContext) -> anyhow::Result<PostProcessorContext>;
}

// ---------------------------------------------------------------------------
// GAIAgentPort — outbound agent delivery
// ---------------------------------------------------------------------------

/// Abstract port for delivering transcription results to an external agent.
///
/// Kept transport-agnostic so adapters can implement CLI, HTTP, etc.
pub trait GAIAgentPort: Send + Sync {
    /// Send a transcription result using the post-processor context.
    fn send_transcription(
        &self,
        ctx: &PostProcessorContext,
    ) -> anyhow::Result<()>;
}
