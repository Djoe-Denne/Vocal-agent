//! Use cases — application-level orchestration.
//!
//! The [`SynthesizeSpeechUseCase`] is the main orchestrator. It coordinates
//! config loading, model resolution, pipeline execution, and TTS inference.

use std::time::Instant;

use crate::domain::models::{SynthesisOptions, SynthesisRequest, SynthesisResult, SynthesisTiming};
use crate::domain::pipeline::{PostProcessorContext, PreProcessorContext};
use crate::domain::ports::TtsEnginePort;
use crate::domain::value_objects::{Language, ModelRef, SampleRate, VoiceId};

use super::config::TtsConfig;
use super::model_resolver::ModelResolver;
use super::pipeline_registry::PipelineRegistry;

/// Main orchestrator for speech synthesis.
///
/// Owns all dependencies and coordinates the full synthesis flow:
///
/// 1. Merge config defaults with request overrides
/// 2. Resolve model reference (HF download or local validation)
/// 3. Validate model variant / voice compatibility
/// 4. Run pre-processors chain
/// 5. Call TTS engine
/// 6. Run post-processors chain
/// 7. Return result
pub struct SynthesizeSpeechUseCase {
    config: TtsConfig,
    model_resolver: ModelResolver,
    engine: Box<dyn TtsEnginePort>,
    pipeline_registry: PipelineRegistry,
}

impl SynthesizeSpeechUseCase {
    /// Create the use case with all injected dependencies.
    pub fn new(
        config: TtsConfig,
        model_resolver: ModelResolver,
        engine: Box<dyn TtsEnginePort>,
        pipeline_registry: PipelineRegistry,
    ) -> Self {
        Self {
            config,
            model_resolver,
            engine,
            pipeline_registry,
        }
    }

    /// Build a [`SynthesisRequest`] from partial overrides, filling in
    /// defaults from config.
    pub fn build_request(
        &self,
        text: String,
        model_ref: Option<ModelRef>,
        voice: Option<VoiceId>,
        language: Option<Language>,
        options: Option<SynthesisOptions>,
        instruct: Option<String>,
        ref_audio_path: Option<std::path::PathBuf>,
        ref_text: Option<String>,
    ) -> SynthesisRequest {
        let defaults = &self.config.defaults;

        SynthesisRequest {
            text,
            model_ref: model_ref.unwrap_or_else(|| defaults.model.clone()),
            voice: voice.unwrap_or_else(|| defaults.voice.clone()),
            language: language.unwrap_or(defaults.language),
            options: options.unwrap_or_else(|| SynthesisOptions {
                temperature: defaults.temperature,
                top_k: defaults.top_k,
                top_p: defaults.top_p,
                repetition_penalty: defaults.repetition_penalty,
                seed: defaults.seed,
                max_frames: defaults.max_frames,
            }),
            instruct,
            ref_audio_path,
            ref_text,
            pre_stages: self.config.pipeline.pre.clone(),
            post_stages: self.config.pipeline.post.clone(),
        }
    }

    /// Execute the full synthesis pipeline.
    pub fn execute(&mut self, request: SynthesisRequest) -> anyhow::Result<SynthesisResult> {
        let total_start = Instant::now();
        let request_id = format!("req_{}", total_start.elapsed().as_nanos());

        // ── 1. Resolve model ──────────────────────────────────────────
        let model_start = Instant::now();
        let resolved_model = self.model_resolver.resolve(&request.model_ref)?;
        let model_load_ms = model_start.elapsed().as_secs_f64() * 1000.0;

        // ── 2. Validate compatibility ─────────────────────────────────
        let mut warnings = Vec::new();
        self.validate_compatibility(&request, &resolved_model.variant, &mut warnings);

        // ── 3. Run pre-processors ─────────────────────────────────────
        let pre_start = Instant::now();
        let mut pre_ctx = PreProcessorContext::new(&request_id, &request.text);

        let pre_chain = self
            .pipeline_registry
            .build_pre_chain(&request.pre_stages)?;

        for processor in &pre_chain {
            pre_ctx = processor.process(pre_ctx)?;
        }
        let preprocess_ms = pre_start.elapsed().as_secs_f64() * 1000.0;

        // Update request text if pre-processors modified it.
        let mut request = request;
        request.text = pre_ctx.text;
        warnings.extend(pre_ctx.warnings);

        // ── 4. Run TTS engine ─────────────────────────────────────────
        let inference_start = Instant::now();
        let mut result = self.engine.synthesize(&resolved_model, &request)?;
        let inference_ms = inference_start.elapsed().as_secs_f64() * 1000.0;

        // ── 5. Run post-processors ────────────────────────────────────
        let post_start = Instant::now();
        let mut post_ctx = PostProcessorContext::new(
            &request_id,
            result.audio_samples,
            result.sample_rate.0,
        );
        post_ctx.artifacts = pre_ctx.artifacts;

        let post_chain = self
            .pipeline_registry
            .build_post_chain(&request.post_stages)?;

        for processor in &post_chain {
            post_ctx = processor.process(post_ctx)?;
        }
        let postprocess_ms = post_start.elapsed().as_secs_f64() * 1000.0;
        warnings.extend(post_ctx.warnings);

        // ── 6. Build final result ─────────────────────────────────────
        let total_ms = total_start.elapsed().as_secs_f64() * 1000.0;

        result.audio_samples = post_ctx.audio_samples;
        result.sample_rate = SampleRate(post_ctx.sample_rate);
        result.warnings = warnings;
        result.timings = SynthesisTiming {
            model_load_ms,
            preprocess_ms,
            inference_ms,
            postprocess_ms,
            total_ms,
        };

        Ok(result)
    }

    /// Check voice/model compatibility and emit warnings.
    fn validate_compatibility(
        &self,
        request: &SynthesisRequest,
        variant: &crate::domain::models::ModelVariant,
        warnings: &mut Vec<String>,
    ) {
        use crate::domain::models::ModelVariant;

        match variant {
            ModelVariant::Base => {
                // Base models support voice cloning, not preset speakers.
                if !matches!(request.voice, VoiceId::Custom(_)) {
                    warnings.push(format!(
                        "Preset voice {:?} used with Base model — \
                         output may be unpredictable. Use a CustomVoice \
                         model for preset speakers.",
                        request.voice
                    ));
                }
            }
            ModelVariant::CustomVoice => {
                // CustomVoice models use preset speakers.
                if matches!(request.voice, VoiceId::Custom(_)) {
                    warnings.push(
                        "Custom voice used with CustomVoice model — \
                         this model only supports preset speakers."
                            .to_owned(),
                    );
                }
                if request.instruct.is_some() {
                    warnings.push(
                        "Voice design instruction ignored — \
                         use a VoiceDesign model for text-described voices."
                            .to_owned(),
                    );
                }
            }
            ModelVariant::VoiceDesign => {
                if request.instruct.is_none() {
                    warnings.push(
                        "VoiceDesign model used without --instruct text — \
                         a voice description is recommended."
                            .to_owned(),
                    );
                }
            }
        }
    }
}
