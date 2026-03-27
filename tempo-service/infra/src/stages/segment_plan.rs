use tempo_domain::convert::ms_to_samples;
use tempo_domain::{DomainError, SegmentPlan, TempoPipelineContext, TempoPipelineStage, WordTiming};

const MIN_SEGMENT_SAMPLES: usize = 160;

/// Step 2: construct treatment segments from word timings.
///
/// Pairs each TTS word timing with the corresponding original timing to derive
/// a target duration and stretch factor (alpha). Segments that are too short
/// or have invalid bounds are filtered out.
pub struct SegmentPlanStage;

impl TempoPipelineStage for SegmentPlanStage {
    fn name(&self) -> &'static str {
        "segment_plan"
    }

    fn execute(&self, context: &mut TempoPipelineContext) -> Result<(), DomainError> {
        if context.tts_timings.is_empty() {
            return Err(DomainError::internal_error(
                "segment_plan: tts_timings is empty",
            ));
        }
        if context.original_timings.is_empty() {
            return Err(DomainError::internal_error(
                "segment_plan: original_timings is empty",
            ));
        }

        let rate = context.sample_rate_hz;
        let total_samples = context.samples.len();
        let mut plans = Vec::new();

        let pairs = pair_timings(&context.tts_timings, &context.original_timings);

        for (tts, original) in &pairs {
            let start_sample = ms_to_samples(tts.start_ms, rate);
            let end_sample = ms_to_samples(tts.end_ms, rate).min(total_samples);

            if end_sample <= start_sample {
                continue;
            }

            let original_duration_samples = end_sample - start_sample;
            if original_duration_samples < MIN_SEGMENT_SAMPLES {
                tracing::trace!(
                    word = %tts.word,
                    duration_samples = original_duration_samples,
                    "segment_plan: skipping too-short segment"
                );
                continue;
            }

            let original_duration_ms = original.end_ms.saturating_sub(original.start_ms);
            let tts_duration_ms = tts.end_ms.saturating_sub(tts.start_ms);

            let target_duration_samples = if original_duration_ms > 0 && tts_duration_ms > 0 {
                ms_to_samples(original_duration_ms, rate)
            } else {
                original_duration_samples
            };

            let alpha = if original_duration_samples > 0 {
                target_duration_samples as f64 / original_duration_samples as f64
            } else {
                1.0
            };

            tracing::debug!(
                segment_index = plans.len(),
                tts_word = %tts.word,
                original_word = %original.word,
                tts_ms = tts_duration_ms,
                original_ms = original_duration_ms,
                delta_ms = original_duration_ms as i64 - tts_duration_ms as i64,
                alpha = alpha,
                start_sample,
                end_sample,
                "segment_plan: word pair mapped"
            );

            plans.push(SegmentPlan {
                start_sample,
                end_sample,
                original_duration_samples,
                target_duration_samples,
                alpha,
            });
        }

        if plans.is_empty() {
            return Err(DomainError::internal_error(
                "segment_plan: no valid segments could be constructed from timings",
            ));
        }

        tracing::debug!(
            segment_count = plans.len(),
            total_samples = total_samples,
            tts_word_count = context.tts_timings.len(),
            original_word_count = context.original_timings.len(),
            "treatment segments constructed"
        );

        context.segment_plans = plans;
        Ok(())
    }
}

/// Pair TTS timings with original timings by index.
/// If counts differ, pair up to the shorter list.
fn pair_timings<'a>(
    tts: &'a [WordTiming],
    original: &'a [WordTiming],
) -> Vec<(&'a WordTiming, &'a WordTiming)> {
    tts.iter().zip(original.iter()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempo_domain::TempoPipelineContext;

    fn word(w: &str, start_ms: u64, end_ms: u64) -> WordTiming {
        WordTiming {
            word: w.to_string(),
            start_ms,
            end_ms,
            confidence: 1.0,
        }
    }

    fn ctx_with_timings(
        tts: Vec<WordTiming>,
        original: Vec<WordTiming>,
        samples_len: usize,
    ) -> TempoPipelineContext {
        TempoPipelineContext::new(vec![0.0; samples_len], 16_000, original, tts)
    }

    #[test]
    fn produces_segment_plans_from_valid_timings() {
        let tts = vec![word("hello", 0, 500), word("world", 500, 1000)];
        let original = vec![word("hello", 0, 600), word("world", 600, 1200)];
        let mut ctx = ctx_with_timings(tts, original, 16_000);
        let stage = SegmentPlanStage;
        stage.execute(&mut ctx).expect("should succeed");
        assert_eq!(ctx.segment_plans.len(), 2);

        let plan0 = &ctx.segment_plans[0];
        assert_eq!(plan0.start_sample, 0);
        assert_eq!(plan0.end_sample, 8_000);
        assert_eq!(plan0.original_duration_samples, 8_000);
        assert_eq!(plan0.target_duration_samples, 9_600);
        assert!((plan0.alpha - 1.2).abs() < 1e-6);
    }

    #[test]
    fn skips_segments_shorter_than_minimum() {
        let tts = vec![word("a", 0, 5), word("hello", 100, 600)];
        let original = vec![word("a", 0, 5), word("hello", 100, 700)];
        let mut ctx = ctx_with_timings(tts, original, 16_000);
        let stage = SegmentPlanStage;
        stage.execute(&mut ctx).expect("should succeed");
        assert_eq!(ctx.segment_plans.len(), 1);
        assert_eq!(ctx.segment_plans[0].start_sample, 1_600);
    }

    #[test]
    fn rejects_empty_tts_timings() {
        let mut ctx = ctx_with_timings(vec![], vec![word("x", 0, 100)], 1_600);
        let stage = SegmentPlanStage;
        assert!(stage.execute(&mut ctx).is_err());
    }

    #[test]
    fn rejects_empty_original_timings() {
        let mut ctx = ctx_with_timings(vec![word("x", 0, 100)], vec![], 1_600);
        let stage = SegmentPlanStage;
        assert!(stage.execute(&mut ctx).is_err());
    }

    #[test]
    fn clamps_end_sample_to_buffer_length() {
        let tts = vec![word("hello", 0, 2000)];
        let original = vec![word("hello", 0, 2000)];
        let mut ctx = ctx_with_timings(tts, original, 8_000);
        let stage = SegmentPlanStage;
        stage.execute(&mut ctx).expect("should succeed");
        assert_eq!(ctx.segment_plans[0].end_sample, 8_000);
    }
}
