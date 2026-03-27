use async_trait::async_trait;
use orchestration_domain::{DomainError, PipelineContext, PipelineStage};
use tempo_domain::{TempoMatchPort, TempoMatchRequest};
use tempo_infra::TempoMatchAdapter;

pub struct TempoMatchStage {
    adapter: TempoMatchAdapter,
}

impl TempoMatchStage {
    pub fn new() -> Self {
        Self {
            adapter: TempoMatchAdapter::new(),
        }
    }
}

impl Default for TempoMatchStage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PipelineStage for TempoMatchStage {
    fn name(&self) -> &'static str {
        "tempo_match"
    }

    async fn execute(&self, context: &mut PipelineContext) -> Result<(), DomainError> {
        let tts_samples = context.audio.samples.clone();
        let tts_sample_rate_hz = context.audio.sample_rate_hz;

        if tts_samples.is_empty() {
            return Err(DomainError::internal_error(
                "tempo_match: audio samples are empty",
            ));
        }

        let tts_timings = map_orch_to_tempo_timings(&context.aligned_words);
        if tts_timings.is_empty() {
            return Err(DomainError::internal_error(
                "tempo_match: no TTS aligned words available",
            ));
        }

        let original_timings = extract_original_timings(context)?;
        if original_timings.is_empty() {
            return Err(DomainError::internal_error(
                "tempo_match: no original timings available in extensions",
            ));
        }

        tracing::debug!(
            tts_sample_count = tts_samples.len(),
            tts_sample_rate_hz,
            tts_timing_count = tts_timings.len(),
            original_timing_count = original_timings.len(),
            "tempo_match: starting tempo adjustment"
        );

        let request = TempoMatchRequest {
            tts_samples,
            tts_sample_rate_hz,
            original_timings,
            tts_timings,
        };

        let output = self.adapter.match_tempo(request).await?;

        tracing::debug!(
            input_samples = context.audio.samples.len(),
            output_samples = output.samples.len(),
            output_sample_rate_hz = output.sample_rate_hz,
            "tempo_match: tempo adjustment complete"
        );

        context.audio.samples = output.samples;
        context.audio.sample_rate_hz = output.sample_rate_hz;

        Ok(())
    }
}

fn map_orch_to_tempo_timings(
    words: &[orchestration_domain::WordTiming],
) -> Vec<tempo_domain::WordTiming> {
    words
        .iter()
        .map(|w| tempo_domain::WordTiming {
            word: w.word.clone(),
            start_ms: w.start_ms,
            end_ms: w.end_ms,
            confidence: w.confidence,
        })
        .collect()
}

fn extract_original_timings(
    context: &PipelineContext,
) -> Result<Vec<tempo_domain::WordTiming>, DomainError> {
    let timings_value = context
        .extension("original.timings")
        .ok_or_else(|| {
            DomainError::internal_error(
                "tempo_match: extension 'original.timings' not found — was snapshot_original_timings stage run?",
            )
        })?;

    let orch_timings: Vec<orchestration_domain::WordTiming> =
        serde_json::from_value(timings_value.clone()).map_err(|err| {
            DomainError::internal_error(&format!(
                "tempo_match: failed to deserialize original.timings: {err}"
            ))
        })?;

    Ok(map_orch_to_tempo_timings(&orch_timings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchestration_domain::{PipelineContext, WordTiming};

    fn make_context_with_data() -> PipelineContext {
        let rate = 16_000u32;
        let freq = 200.0f32;
        let duration_samples = 8000usize; // 500ms
        let samples: Vec<f32> = (0..duration_samples)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin() * 0.5)
            .collect();

        let mut context = PipelineContext::new("test-session", None);
        context.audio.samples = samples;
        context.audio.sample_rate_hz = rate;

        context.aligned_words = vec![
            WordTiming {
                word: "hello".to_string(),
                start_ms: 0,
                end_ms: 250,
                confidence: 0.95,
            },
            WordTiming {
                word: "world".to_string(),
                start_ms: 250,
                end_ms: 500,
                confidence: 0.90,
            },
        ];

        let original_words = vec![
            WordTiming {
                word: "hello".to_string(),
                start_ms: 0,
                end_ms: 300,
                confidence: 0.95,
            },
            WordTiming {
                word: "world".to_string(),
                start_ms: 300,
                end_ms: 650,
                confidence: 0.90,
            },
        ];
        let timings_json = serde_json::to_value(&original_words).expect("serialize");
        context.set_extension("original.timings", timings_json);

        context
    }

    #[tokio::test]
    async fn stage_runs_and_produces_output() {
        let stage = TempoMatchStage::new();
        let mut context = make_context_with_data();
        let input_len = context.audio.samples.len();

        let result = stage.execute(&mut context).await;
        assert!(result.is_ok(), "stage should succeed: {:?}", result.err());
        assert!(!context.audio.samples.is_empty(), "output should not be empty");
        assert_eq!(context.audio.sample_rate_hz, 16_000);
        tracing::info!(
            input_samples = input_len,
            output_samples = context.audio.samples.len(),
            "tempo match test completed"
        );
    }

    #[tokio::test]
    async fn stage_fails_without_audio() {
        let stage = TempoMatchStage::new();
        let mut context = make_context_with_data();
        context.audio.samples.clear();

        let result = stage.execute(&mut context).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn stage_fails_without_tts_timings() {
        let stage = TempoMatchStage::new();
        let mut context = make_context_with_data();
        context.aligned_words.clear();

        let result = stage.execute(&mut context).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn stage_fails_without_original_timings() {
        let stage = TempoMatchStage::new();
        let mut context = make_context_with_data();
        context.take_extension("original.timings");

        let result = stage.execute(&mut context).await;
        assert!(result.is_err());
    }

    #[test]
    fn timing_mapping_preserves_fields() {
        let orch = vec![WordTiming {
            word: "test".to_string(),
            start_ms: 100,
            end_ms: 200,
            confidence: 0.85,
        }];

        let mapped = map_orch_to_tempo_timings(&orch);
        assert_eq!(mapped.len(), 1);
        assert_eq!(mapped[0].word, "test");
        assert_eq!(mapped[0].start_ms, 100);
        assert_eq!(mapped[0].end_ms, 200);
        assert_eq!(mapped[0].confidence, 0.85);
    }
}
