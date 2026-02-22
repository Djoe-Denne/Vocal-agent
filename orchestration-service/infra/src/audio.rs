use orchestration_domain::{DomainError, PipelineContext, PipelineStage};
use async_trait::async_trait;
use serde_json::json;

pub struct AudioPreprocessStage;

impl AudioPreprocessStage {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl PipelineStage for AudioPreprocessStage {
    fn name(&self) -> &'static str {
        "audio-preprocess"
    }

    async fn execute(&self, context: &mut PipelineContext) -> Result<(), DomainError> {
        for sample in &mut context.audio.samples {
            *sample = sample.clamp(-1.0, 1.0);
        }
        Ok(())
    }
}

pub struct ResampleStage {
    target_sample_rate_hz: u32,
}

impl ResampleStage {
    pub fn new(target_sample_rate_hz: u32) -> Self {
        Self {
            target_sample_rate_hz,
        }
    }
}

#[async_trait]
impl PipelineStage for ResampleStage {
    fn name(&self) -> &'static str {
        "resample"
    }

    async fn execute(&self, context: &mut PipelineContext) -> Result<(), DomainError> {
        let source_rate_hz = context.audio.sample_rate_hz;
        if source_rate_hz == 0 || self.target_sample_rate_hz == 0 {
            return Err(DomainError::internal_error(
                "sample rate must be greater than zero",
            ));
        }

        if source_rate_hz == self.target_sample_rate_hz || context.audio.samples.is_empty() {
            context.set_extension("audio.resampled", json!(false));
            return Ok(());
        }

        let resampled = resample_linear(
            &context.audio.samples,
            source_rate_hz,
            self.target_sample_rate_hz,
        );

        tracing::debug!(
            source_rate_hz,
            target_rate_hz = self.target_sample_rate_hz,
            input_samples = context.audio.samples.len(),
            output_samples = resampled.len(),
            "resampled audio for pipeline"
        );

        context.audio.samples = resampled;
        context.audio.sample_rate_hz = self.target_sample_rate_hz;
        context.set_extension("audio.resampled", json!(true));
        context.set_extension("audio.source_sample_rate_hz", json!(source_rate_hz));
        context.set_extension("audio.target_sample_rate_hz", json!(self.target_sample_rate_hz));
        Ok(())
    }
}

fn resample_linear(samples: &[f32], source_rate_hz: u32, target_rate_hz: u32) -> Vec<f32> {
    if source_rate_hz == target_rate_hz {
        return samples.to_vec();
    }
    if samples.len() <= 1 {
        return samples.to_vec();
    }

    let output_len = ((samples.len() as u64 * target_rate_hz as u64) / source_rate_hz as u64)
        .max(1) as usize;
    if output_len <= 1 {
        return vec![samples[0]];
    }

    let mut output = Vec::with_capacity(output_len);
    let max_source_idx = samples.len() - 1;

    for out_idx in 0..output_len {
        let source_pos = out_idx as f64 * source_rate_hz as f64 / target_rate_hz as f64;
        let left_idx = source_pos.floor() as usize;
        let right_idx = (left_idx + 1).min(max_source_idx);
        let frac = (source_pos - left_idx as f64) as f32;

        let left = samples[left_idx];
        let right = samples[right_idx];
        output.push(left * (1.0 - frac) + right * frac);
    }

    output
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

#[cfg(test)]
mod tests {
    use super::{AudioPreprocessStage, ResampleStage};
    use orchestration_domain::{PipelineContext, PipelineStage};

    #[tokio::test]
    async fn audio_preprocess_clamps_samples() {
        let stage = AudioPreprocessStage::new();
        let mut context = PipelineContext::new("session", None);
        context.audio.sample_rate_hz = 16_000;
        context.audio.samples = vec![-2.0, -1.0, 0.0, 1.0, 2.0];

        stage.execute(&mut context).await.expect("stage runs");

        assert_eq!(context.audio.samples, vec![-1.0, -1.0, 0.0, 1.0, 1.0]);
    }

    #[tokio::test]
    async fn resample_stage_changes_sample_rate() {
        let stage = ResampleStage::new(16_000);
        let mut context = PipelineContext::new("session", None);
        context.audio.sample_rate_hz = 48_000;
        context.audio.samples = (0..480).map(|i| i as f32 / 480.0).collect();

        stage.execute(&mut context).await.expect("stage runs");

        assert_eq!(context.audio.sample_rate_hz, 16_000);
        assert!(context.audio.samples.len() < 480);
        assert_eq!(
            context.extension("audio.resampled").and_then(|value| value.as_bool()),
            Some(true)
        );
    }
}
