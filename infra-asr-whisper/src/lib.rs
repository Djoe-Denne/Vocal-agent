use asr_domain::{
    DomainError, DomainEvent, PipelineContext, PipelineStage, Transcript, TranscriptSegment,
    TranscriptToken, TranscriptionOutput, TranscriptionPort, TranscriptionRequest,
};
use async_trait::async_trait;
#[cfg(feature = "whisper-runtime")]
use whisper_rs::{
    DtwMode, DtwModelPreset, DtwParameters, FullParams, SamplingStrategy, WhisperContext,
    WhisperContextParameters,
};

#[derive(Debug, Clone)]
pub struct WhisperAdapterConfig {
    pub model_path: String,
    pub language: String,
    pub temperature: f32,
    pub threads: usize,
    pub dtw_preset: String,
    pub dtw_mem_size: usize,
}

impl WhisperAdapterConfig {
    #[cfg(feature = "whisper-runtime")]
    fn to_dtw_preset(&self) -> DtwModelPreset {
        match self.dtw_preset.to_ascii_lowercase().as_str() {
            "tiny_en" => DtwModelPreset::TinyEn,
            "tiny" => DtwModelPreset::Tiny,
            "base_en" => DtwModelPreset::BaseEn,
            "base" => DtwModelPreset::Base,
            "small_en" => DtwModelPreset::SmallEn,
            "small" => DtwModelPreset::Small,
            "medium_en" => DtwModelPreset::MediumEn,
            "medium" => DtwModelPreset::Medium,
            "large_v1" => DtwModelPreset::LargeV1,
            "large_v2" => DtwModelPreset::LargeV2,
            "large_v3" => DtwModelPreset::LargeV3,
            "large_v3_turbo" => DtwModelPreset::LargeV3Turbo,
            _ => DtwModelPreset::Base,
        }
    }
}

pub struct WhisperTranscriptionAdapter {
    config: WhisperAdapterConfig,
}

impl WhisperTranscriptionAdapter {
    pub fn new(config: WhisperAdapterConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl TranscriptionPort for WhisperTranscriptionAdapter {
    async fn transcribe(
        &self,
        request: TranscriptionRequest,
    ) -> Result<TranscriptionOutput, DomainError> {
        #[cfg(feature = "whisper-runtime")]
        {
        let mut context_params = WhisperContextParameters::default();
        context_params.dtw_parameters = DtwParameters {
            mode: DtwMode::ModelPreset {
                model_preset: self.config.to_dtw_preset(),
            },
            dtw_mem_size: self.config.dtw_mem_size,
        };

        let whisper_context = WhisperContext::new_with_params(&self.config.model_path, context_params)
            .map_err(|err| DomainError::Transcription(format!("failed to load model: {err}")))?;
        let mut state = whisper_context
            .create_state()
            .map_err(|err| DomainError::Transcription(format!("failed to create state: {err}")))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(self.config.threads as i32);
        params.set_language(Some(self.config.language.as_str()));
        params.set_no_timestamps(false);
        params.set_token_timestamps(true);
        params.set_temperature(self.config.temperature);
        params.set_single_segment(false);
        params.set_print_realtime(false);
        params.set_print_progress(false);
        params.set_print_timestamps(false);

        state
            .full(params, &request.audio.samples)
            .map_err(|err| DomainError::Transcription(format!("full decode failed: {err}")))?;

        let mut segments = Vec::new();
        for idx in 0..state.full_n_segments() {
            let Some(segment) = state.get_segment(idx) else {
                continue;
            };
            let start_ms = (segment.start_timestamp() as u64) * 10;
            let end_ms = (segment.end_timestamp() as u64) * 10;
            let text = segment
                .to_str_lossy()
                .map(|cow| cow.to_string())
                .unwrap_or_default();

            // DTW is enabled in context parameters. We derive stable token windows from segment span.
            let n_tokens = segment.n_tokens().max(0) as usize;
            let token_span = if n_tokens > 0 {
                (end_ms.saturating_sub(start_ms) / n_tokens as u64).max(1)
            } else {
                1
            };
            let mut tokens = Vec::new();
            for token_idx in 0..(segment.n_tokens().max(0)) {
                let Some(token) = segment.get_token(token_idx) else {
                    continue;
                };
                let token_text = token
                    .to_str_lossy()
                    .map(|cow| cow.to_string())
                    .unwrap_or_default();
                let offset = token_idx as u64 * token_span;
                tokens.push(TranscriptToken {
                    text: token_text,
                    start_ms: start_ms.saturating_add(offset),
                    end_ms: (start_ms.saturating_add(offset).saturating_add(token_span)).min(end_ms),
                    confidence: token.token_probability(),
                });
            }

            segments.push(TranscriptSegment {
                text,
                start_ms,
                end_ms,
                tokens,
            });
        }

        Ok(TranscriptionOutput {
            transcript: Transcript {
                language: request
                    .language_hint
                    .unwrap_or(asr_domain::LanguageTag::Auto),
                segments,
            },
        })
        }

        #[cfg(not(feature = "whisper-runtime"))]
        {
            let _ = &self.config;
            let transcript = Transcript {
                language: request
                    .language_hint
                    .unwrap_or(asr_domain::LanguageTag::Auto),
                segments: vec![TranscriptSegment {
                    text: "whisper-runtime feature disabled".to_string(),
                    start_ms: 0,
                    end_ms: 0,
                    tokens: vec![TranscriptToken {
                        text: "whisper-runtime".to_string(),
                        start_ms: 0,
                        end_ms: 0,
                        confidence: 1.0,
                    }],
                }],
            };
            Ok(TranscriptionOutput { transcript })
        }
    }
}

pub struct WhisperTranscriptionStage {
    adapter: Box<dyn TranscriptionPort>,
}

impl WhisperTranscriptionStage {
    pub fn new(adapter: Box<dyn TranscriptionPort>) -> Self {
        Self { adapter }
    }
}

#[async_trait]
impl PipelineStage for WhisperTranscriptionStage {
    fn name(&self) -> &'static str {
        "transcription-whisper"
    }

    async fn execute(&self, context: &mut PipelineContext) -> Result<(), DomainError> {
        let output = self
            .adapter
            .transcribe(TranscriptionRequest {
                language_hint: context.language_hint.clone(),
                audio: context.audio.clone(),
            })
            .await?;
        context.transcript = Some(output.transcript.clone());
        context.events.push(DomainEvent::FinalTranscript {
            transcript: output.transcript,
        });
        Ok(())
    }
}
