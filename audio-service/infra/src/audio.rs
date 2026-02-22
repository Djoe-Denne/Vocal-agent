use async_trait::async_trait;
use audio_domain::{
    AudioTransformPort, AudioTransformRequest, AudioTransformResult, DomainError, TransformMetadata,
};

#[derive(Default)]
pub struct AudioTransformerAdapter;

impl AudioTransformerAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl AudioTransformPort for AudioTransformerAdapter {
    async fn transform(
        &self,
        request: AudioTransformRequest,
    ) -> Result<AudioTransformResult, DomainError> {
        if request.source_sample_rate_hz == 0 || request.target_sample_rate_hz == 0 {
            return Err(DomainError::internal_error(
                "sample rate must be greater than zero",
            ));
        }

        let input_sample_count = request.samples.len();
        let mut samples = request.samples;
        let clamped = clamp_samples(&mut samples);
        let should_resample =
            request.source_sample_rate_hz != request.target_sample_rate_hz && !samples.is_empty();

        if should_resample {
            samples = resample_linear(
                &samples,
                request.source_sample_rate_hz,
                request.target_sample_rate_hz,
            );
        }

        let output_sample_count = samples.len();
        let metadata = TransformMetadata {
            clamped,
            resampled: should_resample,
            input_sample_count,
            output_sample_count,
            source_sample_rate_hz: request.source_sample_rate_hz,
            target_sample_rate_hz: request.target_sample_rate_hz,
        };

        tracing::debug!(
            source_sample_rate_hz = metadata.source_sample_rate_hz,
            target_sample_rate_hz = metadata.target_sample_rate_hz,
            input_samples = metadata.input_sample_count,
            output_samples = metadata.output_sample_count,
            clamped = metadata.clamped,
            resampled = metadata.resampled,
            "audio transformation completed"
        );

        Ok(AudioTransformResult {
            samples,
            sample_rate_hz: request.target_sample_rate_hz,
            metadata,
        })
    }
}

fn clamp_samples(samples: &mut [f32]) -> bool {
    let mut clamped_any = false;
    for sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        if clamped != *sample {
            clamped_any = true;
            *sample = clamped;
        }
    }
    clamped_any
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
    use super::AudioTransformerAdapter;
    use audio_domain::{AudioTransformPort, AudioTransformRequest};

    #[tokio::test]
    async fn transform_clamps_samples() {
        let adapter = AudioTransformerAdapter::new();
        let result = adapter
            .transform(AudioTransformRequest {
                samples: vec![-2.0, -1.0, 0.0, 1.0, 2.0],
                source_sample_rate_hz: 16_000,
                target_sample_rate_hz: 16_000,
            })
            .await
            .expect("adapter runs");

        assert_eq!(result.samples, vec![-1.0, -1.0, 0.0, 1.0, 1.0]);
        assert!(result.metadata.clamped);
        assert!(!result.metadata.resampled);
    }

    #[tokio::test]
    async fn transform_resamples_audio() {
        let adapter = AudioTransformerAdapter::new();
        let result = adapter
            .transform(AudioTransformRequest {
                samples: (0..480).map(|i| i as f32 / 480.0).collect(),
                source_sample_rate_hz: 48_000,
                target_sample_rate_hz: 16_000,
            })
            .await
            .expect("adapter runs");

        assert_eq!(result.sample_rate_hz, 16_000);
        assert!(result.samples.len() < 480);
        assert!(result.metadata.resampled);
    }
}
