use asr_domain::{
    DomainError, Transcript, TranscriptSegment, TranscriptToken, TranscriptionOutput,
    TranscriptionPort, TranscriptionRequest,
};
use async_trait::async_trait;
use std::sync::Mutex;
use whisper_rs::{
    DtwMode, DtwModelPreset, DtwParameters, FullParams, SamplingStrategy, WhisperContext,
    WhisperContextParameters, WhisperTokenData,
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

fn resolve_decode_language(
    config_language: &str,
    hint: Option<&asr_domain::LanguageTag>,
) -> Option<String> {
    if let Some(tag) = hint {
        return match tag {
            asr_domain::LanguageTag::Fr => Some("fr".to_string()),
            asr_domain::LanguageTag::En => Some("en".to_string()),
            asr_domain::LanguageTag::Auto => None,
            asr_domain::LanguageTag::Other(code) => {
                let normalized = code.trim().to_ascii_lowercase();
                if normalized.is_empty() {
                    None
                } else {
                    Some(normalized)
                }
            }
        };
    }

    let normalized = config_language.trim().to_ascii_lowercase();
    if normalized.is_empty() || normalized == "auto" {
        None
    } else {
        Some(normalized)
    }
}

fn to_ms_10ms_units(raw: i64) -> Option<u64> {
    let raw_u64 = u64::try_from(raw).ok()?;
    raw_u64.checked_mul(10)
}

fn token_start_hint_ms(token_data: WhisperTokenData) -> Option<u64> {
    to_ms_10ms_units(token_data.t_dtw).or_else(|| to_ms_10ms_units(token_data.t0))
}

fn token_end_hint_ms(token_data: WhisperTokenData) -> Option<u64> {
    to_ms_10ms_units(token_data.t1)
}

pub struct WhisperTranscriptionAdapter {
    config: WhisperAdapterConfig,
    runtime: Mutex<WhisperRuntime>,
}

struct WhisperRuntime {
    context: Option<WhisperContext>,
}

impl WhisperTranscriptionAdapter {
    pub fn new(config: WhisperAdapterConfig) -> Self {
        Self {
            config,
            runtime: Mutex::new(WhisperRuntime { context: None }),
        }
    }
}

#[async_trait]
impl TranscriptionPort for WhisperTranscriptionAdapter {
    async fn transcribe(
        &self,
        request: TranscriptionRequest,
    ) -> Result<TranscriptionOutput, DomainError> {
        self.transcribe_with_runtime(request)
    }
}

impl WhisperTranscriptionAdapter {
    fn to_dtw_preset(&self) -> DtwModelPreset {
        self.config.to_dtw_preset()
    }

    fn transcribe_with_runtime(
        &self,
        request: TranscriptionRequest,
    ) -> Result<TranscriptionOutput, DomainError> {
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| DomainError::internal_error("whisper runtime lock poisoned"))?;

        if runtime.context.is_none() {
            let mut context_params = WhisperContextParameters::default();
            context_params.dtw_parameters = DtwParameters {
                mode: DtwMode::ModelPreset {
                    model_preset: self.to_dtw_preset(),
                },
                dtw_mem_size: self.config.dtw_mem_size,
            };

            let whisper_context =
                WhisperContext::new_with_params(&self.config.model_path, context_params).map_err(
                    |err| {
                        DomainError::external_service_error(
                            "whisper",
                            &format!("failed to load model: {err}"),
                        )
                    },
                )?;
            runtime.context = Some(whisper_context);
        }

        let whisper_context = runtime
            .context
            .as_ref()
            .ok_or_else(|| DomainError::internal_error("whisper context unavailable"))?;

        let mut state = whisper_context.create_state().map_err(|err| {
            DomainError::external_service_error(
                "whisper",
                &format!("failed to create state: {err}"),
            )
        })?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(self.config.threads as i32);
        let decode_language =
            resolve_decode_language(&self.config.language, request.language_hint.as_ref());
        params.set_language(decode_language.as_deref());
        params.set_no_timestamps(false);
        params.set_token_timestamps(true);
        params.set_split_on_word(true);
        params.set_temperature(self.config.temperature);
        params.set_single_segment(false);
        params.set_print_realtime(false);
        params.set_print_progress(false);
        params.set_print_timestamps(false);

        state.full(params, &request.audio.samples).map_err(|err| {
            DomainError::external_service_error("whisper", &format!("full decode failed: {err}"))
        })?;

        let mut segments = Vec::new();
        for idx in 0..state.full_n_segments() {
            let Some(segment) = state.get_segment(idx) else {
                continue;
            };
            let start_ms = to_ms_10ms_units(segment.start_timestamp()).unwrap_or(0);
            let end_ms = to_ms_10ms_units(segment.end_timestamp()).unwrap_or(start_ms);
            let text = segment
                .to_str_lossy()
                .map(|cow| cow.to_string())
                .unwrap_or_default();

            let n_tokens = segment.n_tokens().max(0) as usize;
            let token_span = if n_tokens > 0 {
                (end_ms.saturating_sub(start_ms) / n_tokens as u64).max(1)
            } else {
                1
            };

            let mut raw_tokens = Vec::new();
            for token_idx in 0..segment.n_tokens().max(0) {
                let Some(token) = segment.get_token(token_idx) else {
                    continue;
                };
                let token_text = token
                    .to_str_lossy()
                    .map(|cow| cow.to_string())
                    .unwrap_or_default();
                let token_data = token.token_data();
                raw_tokens.push((
                    token_text,
                    token.token_probability(),
                    token_start_hint_ms(token_data),
                    token_end_hint_ms(token_data),
                ));
            }

            let mut tokens = Vec::new();
            for (idx, (text, confidence, start_hint_ms, end_hint_ms)) in
                raw_tokens.iter().enumerate()
            {
                let fallback_start_ms = start_ms.saturating_add(idx as u64 * token_span);
                let fallback_end_ms = fallback_start_ms.saturating_add(token_span).min(end_ms);
                let next_start_hint_ms = raw_tokens.get(idx + 1).and_then(|raw| raw.2);

                let token_start_ms = start_hint_ms
                    .unwrap_or(fallback_start_ms)
                    .clamp(start_ms, end_ms);
                let mut token_end_ms = end_hint_ms
                    .filter(|end| *end > token_start_ms)
                    .or_else(|| next_start_hint_ms.filter(|next| *next > token_start_ms))
                    .unwrap_or(fallback_end_ms);

                let min_end = token_start_ms.saturating_add(1);
                let max_end = end_ms.max(min_end);
                token_end_ms = token_end_ms.clamp(min_end, max_end);

                tokens.push(TranscriptToken {
                    text: text.clone(),
                    start_ms: token_start_ms,
                    end_ms: token_end_ms,
                    confidence: *confidence,
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
}
