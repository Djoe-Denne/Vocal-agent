use tempo_domain::{DomainError, TempoPipelineContext, TempoPipelineStage};

const MIN_SAMPLE_RATE_HZ: u32 = 8_000;
const MAX_SAMPLE_RATE_HZ: u32 = 192_000;

/// Step 1: validate and prepare the audio buffer.
///
/// Ensures sample rate is within expected bounds, the buffer is non-empty,
/// and samples are clamped to [-1.0, 1.0].
pub struct AudioPrepareStage;

impl TempoPipelineStage for AudioPrepareStage {
    fn name(&self) -> &'static str {
        "audio_prepare"
    }

    fn execute(&self, context: &mut TempoPipelineContext) -> Result<(), DomainError> {
        if context.samples.is_empty() {
            return Err(DomainError::internal_error(
                "audio_prepare: input sample buffer is empty",
            ));
        }

        if context.sample_rate_hz < MIN_SAMPLE_RATE_HZ
            || context.sample_rate_hz > MAX_SAMPLE_RATE_HZ
        {
            return Err(DomainError::internal_error(&format!(
                "audio_prepare: sample rate {} Hz is outside acceptable range [{}, {}]",
                context.sample_rate_hz, MIN_SAMPLE_RATE_HZ, MAX_SAMPLE_RATE_HZ,
            )));
        }

        let mut clamped_count = 0usize;
        for sample in &mut context.samples {
            if !sample.is_finite() {
                *sample = 0.0;
                clamped_count += 1;
            } else if *sample > 1.0 || *sample < -1.0 {
                *sample = sample.clamp(-1.0, 1.0);
                clamped_count += 1;
            }
        }

        tracing::debug!(
            sample_count = context.samples.len(),
            sample_rate_hz = context.sample_rate_hz,
            clamped_count,
            "audio buffer validated"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempo_domain::TempoPipelineContext;

    fn ctx(samples: Vec<f32>, rate: u32) -> TempoPipelineContext {
        TempoPipelineContext::new(samples, rate, Vec::new(), Vec::new())
    }

    #[test]
    fn rejects_empty_buffer() {
        let stage = AudioPrepareStage;
        let mut c = ctx(vec![], 16_000);
        assert!(stage.execute(&mut c).is_err());
    }

    #[test]
    fn rejects_low_sample_rate() {
        let stage = AudioPrepareStage;
        let mut c = ctx(vec![0.1], 100);
        assert!(stage.execute(&mut c).is_err());
    }

    #[test]
    fn rejects_high_sample_rate() {
        let stage = AudioPrepareStage;
        let mut c = ctx(vec![0.1], 500_000);
        assert!(stage.execute(&mut c).is_err());
    }

    #[test]
    fn clamps_out_of_range_samples() {
        let stage = AudioPrepareStage;
        let mut c = ctx(vec![-2.0, 0.5, 1.5, f32::NAN], 16_000);
        stage.execute(&mut c).expect("should succeed");
        assert_eq!(c.samples[0], -1.0);
        assert_eq!(c.samples[1], 0.5);
        assert_eq!(c.samples[2], 1.0);
        assert_eq!(c.samples[3], 0.0);
    }

    #[test]
    fn accepts_valid_input() {
        let stage = AudioPrepareStage;
        let mut c = ctx(vec![0.0, 0.1, -0.3], 22_050);
        stage.execute(&mut c).expect("should succeed");
        assert_eq!(c.samples, vec![0.0, 0.1, -0.3]);
    }
}
