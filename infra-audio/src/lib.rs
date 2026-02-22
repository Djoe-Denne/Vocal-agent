use asr_domain::{DomainError, PipelineContext, PipelineStage};
use async_trait::async_trait;

pub struct AudioPreprocessStage {
    target_sample_rate_hz: u32,
}

impl AudioPreprocessStage {
    pub fn new(target_sample_rate_hz: u32) -> Self {
        Self {
            target_sample_rate_hz,
        }
    }
}

#[async_trait]
impl PipelineStage for AudioPreprocessStage {
    fn name(&self) -> &'static str {
        "audio-preprocess"
    }

    async fn execute(&self, context: &mut PipelineContext) -> Result<(), DomainError> {
        // Keep this stage deterministic and minimal: clamp values and enforce target sample rate.
        for sample in &mut context.audio.samples {
            *sample = sample.clamp(-1.0, 1.0);
        }
        context.audio.sample_rate_hz = self.target_sample_rate_hz;
        Ok(())
    }
}

pub fn pcm16le_bytes_to_f32(samples: &[u8]) -> Vec<f32> {
    samples
        .chunks_exact(2)
        .map(|chunk| {
            let value = i16::from_le_bytes([chunk[0], chunk[1]]);
            f32::from(value) / f32::from(i16::MAX)
        })
        .collect()
}
