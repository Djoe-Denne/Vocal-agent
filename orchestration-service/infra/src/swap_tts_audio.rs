use async_trait::async_trait;
use orchestration_domain::{DomainError, PipelineContext, PipelineStage};

use crate::audio::resample_linear;

pub struct SwapTtsAudioStage;

impl SwapTtsAudioStage {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl PipelineStage for SwapTtsAudioStage {
    fn name(&self) -> &'static str {
        "swap_tts_audio"
    }

    async fn execute(&self, context: &mut PipelineContext) -> Result<(), DomainError> {
        let tts_output = context.tts_output.as_ref().ok_or_else(|| {
            DomainError::internal_error("swap_tts_audio requires tts_output to be present")
        })?;

        let target_rate = context.audio.sample_rate_hz;
        let tts_rate = tts_output.sample_rate_hz;

        let (samples, rate) = if tts_rate != target_rate && target_rate > 0 {
            let resampled = resample_linear(&tts_output.samples, tts_rate, target_rate);
            tracing::debug!(
                original_sample_count = context.audio.samples.len(),
                original_sample_rate_hz = target_rate,
                tts_sample_count = tts_output.samples.len(),
                tts_sample_rate_hz = tts_rate,
                resampled_sample_count = resampled.len(),
                "swapping context audio with resampled TTS output"
            );
            (resampled, target_rate)
        } else {
            tracing::debug!(
                original_sample_count = context.audio.samples.len(),
                original_sample_rate_hz = target_rate,
                tts_sample_count = tts_output.samples.len(),
                tts_sample_rate_hz = tts_rate,
                "swapping context audio with TTS output (same rate)"
            );
            (tts_output.samples.clone(), tts_rate)
        };

        context.audio.samples = samples;
        context.audio.sample_rate_hz = rate;

        context.transcript = None;
        context.aligned_words.clear();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchestration_domain::{PipelineContext, TtsOutput, WordTiming};

    #[tokio::test]
    async fn swap_resamples_to_original_rate_and_clears_state() {
        let stage = SwapTtsAudioStage::new();
        let mut context = PipelineContext::new("session", None);
        context.audio.samples = vec![1.0; 160];
        context.audio.sample_rate_hz = 16_000;
        context.aligned_words = vec![WordTiming {
            word: "test".to_string(),
            start_ms: 0,
            end_ms: 100,
            confidence: 0.9,
        }];
        context.tts_output = Some(TtsOutput {
            samples: vec![0.5; 240],
            sample_rate_hz: 24_000,
            word_timings: vec![],
        });

        stage.execute(&mut context).await.expect("stage runs");

        assert_eq!(context.audio.sample_rate_hz, 16_000);
        assert_eq!(context.audio.samples.len(), 160);
        assert!(context.transcript.is_none());
        assert!(context.aligned_words.is_empty());
    }

    #[tokio::test]
    async fn swap_keeps_rate_when_already_matching() {
        let stage = SwapTtsAudioStage::new();
        let mut context = PipelineContext::new("session", None);
        context.audio.samples = vec![1.0, 2.0, 3.0];
        context.audio.sample_rate_hz = 16_000;
        context.tts_output = Some(TtsOutput {
            samples: vec![0.5, 0.6],
            sample_rate_hz: 16_000,
            word_timings: vec![],
        });

        stage.execute(&mut context).await.expect("stage runs");

        assert_eq!(context.audio.samples, vec![0.5, 0.6]);
        assert_eq!(context.audio.sample_rate_hz, 16_000);
    }

    #[tokio::test]
    async fn swap_fails_without_tts_output() {
        let stage = SwapTtsAudioStage::new();
        let mut context = PipelineContext::new("session", None);

        let result = stage.execute(&mut context).await;
        assert!(result.is_err());
    }
}
