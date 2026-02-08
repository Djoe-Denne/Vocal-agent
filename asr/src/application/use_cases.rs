//! Use cases — application-level orchestration.
//!
//! The [`TranscribeAudioUseCase`] is the main orchestrator. It coordinates
//! config loading, model resolution, pipeline execution, and ASR inference.

use std::path::PathBuf;
use std::time::Instant;

use crate::domain::models::{
    TranscriptionOptions, TranscriptionRequest, TranscriptionResult, TranscriptionTiming,
};
use crate::domain::pipeline::{PostProcessorContext, PreProcessorContext};
use crate::domain::ports::AsrEnginePort;
use crate::domain::value_objects::{Language, ModelRef};

use super::config::AsrConfig;
use super::model_resolver::ModelResolver;
use super::pipeline_registry::PipelineRegistry;

/// Main orchestrator for audio transcription.
///
/// Owns all dependencies and coordinates the full transcription flow:
///
/// 1. Merge config defaults with request overrides
/// 2. Resolve model reference (local validation)
/// 3. Run pre-processors chain
/// 4. Call ASR engine
/// 5. Run post-processors chain
/// 6. Return result
pub struct TranscribeAudioUseCase {
    config: AsrConfig,
    model_resolver: ModelResolver,
    engine: Box<dyn AsrEnginePort>,
    pipeline_registry: PipelineRegistry,
}

impl TranscribeAudioUseCase {
    /// Create the use case with all injected dependencies.
    pub fn new(
        config: AsrConfig,
        model_resolver: ModelResolver,
        engine: Box<dyn AsrEnginePort>,
        pipeline_registry: PipelineRegistry,
    ) -> Self {
        Self {
            config,
            model_resolver,
            engine,
            pipeline_registry,
        }
    }

    /// Build a [`TranscriptionRequest`] from partial overrides, filling in
    /// defaults from config.
    pub fn build_request(
        &self,
        audio_path: PathBuf,
        model_ref: Option<ModelRef>,
        language: Option<Language>,
    ) -> TranscriptionRequest {
        let defaults = &self.config.defaults;

        let effective_model_ref = model_ref.unwrap_or_else(|| ModelRef::Local {
            path: self.config.engine.model_dir.clone(),
        });

        TranscriptionRequest {
            audio_path,
            model_ref: effective_model_ref,
            options: TranscriptionOptions {
                language: language.unwrap_or(defaults.language),
            },
            pre_stages: self.config.pipeline.pre.clone(),
            post_stages: self.config.pipeline.post.clone(),
        }
    }

    /// Execute the full transcription pipeline.
    pub fn execute(
        &mut self,
        request: TranscriptionRequest,
    ) -> anyhow::Result<TranscriptionResult> {
        let total_start = Instant::now();
        let request_id = format!("req_{}", total_start.elapsed().as_nanos());

        // -- 1. Resolve model ------------------------------------------------
        let model_start = Instant::now();
        let resolved_model = self.model_resolver.resolve(&request.model_ref)?;
        let model_load_ms = model_start.elapsed().as_secs_f64() * 1000.0;

        // -- 2. Run pre-processors -------------------------------------------
        let pre_start = Instant::now();
        let mut pre_ctx =
            PreProcessorContext::new(&request_id, &request.audio_path);

        let pre_chain = self
            .pipeline_registry
            .build_pre_chain(&request.pre_stages)?;

        for processor in &pre_chain {
            pre_ctx = processor.process(pre_ctx)?;
        }
        let preprocess_ms = pre_start.elapsed().as_secs_f64() * 1000.0;

        // Update request audio path if pre-processors modified it.
        let mut request = request;
        request.audio_path = pre_ctx.audio_path;
        let mut warnings: Vec<String> = pre_ctx.warnings;

        // -- 3. Run ASR engine -----------------------------------------------
        let inference_start = Instant::now();
        let mut result = self.engine.transcribe(&resolved_model, &request)?;
        let inference_ms = inference_start.elapsed().as_secs_f64() * 1000.0;

        // -- 4. Run post-processors ------------------------------------------
        let post_start = Instant::now();
        let mut post_ctx = PostProcessorContext::new(&request_id, &result.text);
        post_ctx.artifacts = pre_ctx.artifacts;

        let post_chain = self
            .pipeline_registry
            .build_post_chain(&request.post_stages)?;

        for processor in &post_chain {
            post_ctx = processor.process(post_ctx)?;
        }
        let postprocess_ms = post_start.elapsed().as_secs_f64() * 1000.0;
        warnings.extend(post_ctx.warnings);

        // -- 5. Build final result -------------------------------------------
        let total_ms = total_start.elapsed().as_secs_f64() * 1000.0;

        result.text = post_ctx.text;
        result.warnings = warnings;
        result.timings = TranscriptionTiming {
            model_load_ms,
            preprocess_ms,
            inference_ms,
            postprocess_ms,
            total_ms,
        };

        Ok(result)
    }
}
