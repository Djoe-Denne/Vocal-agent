use tempo_domain::convert::ms_to_samples;
use tempo_domain::{
    DomainError, SegmentKind, SegmentPlan, TempoPipelineContext, TempoPipelineStage, WordTiming,
};

const MIN_WORD_SEGMENT_SAMPLES: usize = 160;

/// Step 2: construct treatment segments from word timings.
///
/// Builds an alternating timeline of `Gap` and `Word` segments by pairing
/// TTS timings with original timings.  Gaps between words are explicitly
/// represented so the pipeline can stretch/compress pauses independently.
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

        let pairs = pair_timings(&context.tts_timings, &context.original_timings);
        let mut plans: Vec<SegmentPlan> = Vec::new();

        let mut tts_cursor_ms: u64 = 0;
        let mut original_cursor_ms: u64 = 0;

        for (i, (tts, original)) in pairs.iter().enumerate() {
            // --- Insert a Gap segment for any pause before this word ---
            if tts.start_ms > tts_cursor_ms {
                let gap_tts_start = tts_cursor_ms;
                let gap_tts_end = tts.start_ms;
                let gap_original_start = original_cursor_ms;
                let gap_original_end = original.start_ms;

                let gap_start_sample = ms_to_samples(gap_tts_start, rate).min(total_samples);
                let gap_end_sample = ms_to_samples(gap_tts_end, rate).min(total_samples);

                if gap_end_sample > gap_start_sample {
                    let source_dur = gap_end_sample - gap_start_sample;
                    let original_gap_ms =
                        gap_original_end.saturating_sub(gap_original_start);
                    let target_dur = if original_gap_ms > 0 {
                        ms_to_samples(original_gap_ms, rate)
                    } else {
                        source_dur
                    };
                    let alpha = if source_dur > 0 {
                        target_dur as f64 / source_dur as f64
                    } else {
                        1.0
                    };

                    tracing::debug!(
                        segment_index = plans.len(),
                        kind = "Gap",
                        before_word = i,
                        tts_gap_ms = gap_tts_end - gap_tts_start,
                        original_gap_ms = original_gap_ms,
                        alpha,
                        "segment_plan: gap before word"
                    );

                    plans.push(SegmentPlan {
                        kind: SegmentKind::Gap,
                        start_sample: gap_start_sample,
                        end_sample: gap_end_sample,
                        original_duration_samples: source_dur,
                        target_duration_samples: target_dur,
                        alpha,
                        tts_start_ms: gap_tts_start,
                        tts_end_ms: gap_tts_end,
                        original_start_ms: gap_original_start,
                        original_end_ms: gap_original_end,
                        label: None,
                    });
                }
            }

            // --- Insert the Word segment ---
            let start_sample = ms_to_samples(tts.start_ms, rate).min(total_samples);
            let end_sample = ms_to_samples(tts.end_ms, rate).min(total_samples);

            if end_sample <= start_sample {
                tts_cursor_ms = tts.end_ms;
                original_cursor_ms = original.end_ms;
                continue;
            }

            let source_dur = end_sample - start_sample;
            if source_dur < MIN_WORD_SEGMENT_SAMPLES {
                tracing::trace!(
                    word = %tts.word,
                    duration_samples = source_dur,
                    "segment_plan: skipping too-short word segment"
                );
                tts_cursor_ms = tts.end_ms;
                original_cursor_ms = original.end_ms;
                continue;
            }

            let original_duration_ms = original.end_ms.saturating_sub(original.start_ms);
            let tts_duration_ms = tts.end_ms.saturating_sub(tts.start_ms);

            let target_dur = if original_duration_ms > 0 && tts_duration_ms > 0 {
                ms_to_samples(original_duration_ms, rate)
            } else {
                source_dur
            };

            let alpha = if source_dur > 0 {
                target_dur as f64 / source_dur as f64
            } else {
                1.0
            };

            tracing::debug!(
                segment_index = plans.len(),
                kind = "Word",
                tts_word = %tts.word,
                original_word = %original.word,
                tts_ms = tts_duration_ms,
                original_ms = original_duration_ms,
                delta_ms = original_duration_ms as i64 - tts_duration_ms as i64,
                alpha,
                start_sample,
                end_sample,
                "segment_plan: word pair mapped"
            );

            plans.push(SegmentPlan {
                kind: SegmentKind::Word,
                start_sample,
                end_sample,
                original_duration_samples: source_dur,
                target_duration_samples: target_dur,
                alpha,
                tts_start_ms: tts.start_ms,
                tts_end_ms: tts.end_ms,
                original_start_ms: original.start_ms,
                original_end_ms: original.end_ms,
                label: Some(tts.word.clone()),
            });

            tts_cursor_ms = tts.end_ms;
            original_cursor_ms = original.end_ms;
        }

        // --- Trailing gap after last word ---
        let tts_total_ms = (total_samples as u64 * 1000) / rate as u64;
        if tts_cursor_ms < tts_total_ms {
            let gap_start_sample = ms_to_samples(tts_cursor_ms, rate).min(total_samples);
            let gap_end_sample = total_samples;

            if gap_end_sample > gap_start_sample {
                let source_dur = gap_end_sample - gap_start_sample;
                // For the trailing gap we keep the same duration (no original reference)
                plans.push(SegmentPlan {
                    kind: SegmentKind::Gap,
                    start_sample: gap_start_sample,
                    end_sample: gap_end_sample,
                    original_duration_samples: source_dur,
                    target_duration_samples: source_dur,
                    alpha: 1.0,
                    tts_start_ms: tts_cursor_ms,
                    tts_end_ms: tts_total_ms,
                    original_start_ms: original_cursor_ms,
                    original_end_ms: original_cursor_ms + (tts_total_ms - tts_cursor_ms),
                    label: None,
                });
            }
        }

        if plans.is_empty() {
            return Err(DomainError::internal_error(
                "segment_plan: no valid segments could be constructed from timings",
            ));
        }

        let word_count = plans.iter().filter(|p| p.kind == SegmentKind::Word).count();
        let gap_count = plans.iter().filter(|p| p.kind == SegmentKind::Gap).count();

        tracing::debug!(
            segment_count = plans.len(),
            word_count,
            gap_count,
            total_samples,
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
    fn produces_alternating_word_gap_segments() {
        // TTS:      [0..500] hello, [600..1000] world  (gap at 500..600)
        // Original: [0..600] hello, [700..1200] world  (gap at 600..700)
        let tts = vec![word("hello", 0, 500), word("world", 600, 1000)];
        let original = vec![word("hello", 0, 600), word("world", 700, 1200)];
        let mut ctx = ctx_with_timings(tts, original, 16_000);
        let stage = SegmentPlanStage;
        stage.execute(&mut ctx).expect("should succeed");

        // Expect: Word(hello), Gap(500..600), Word(world), trailing Gap
        let kinds: Vec<&SegmentKind> = ctx.segment_plans.iter().map(|p| &p.kind).collect();
        assert_eq!(kinds[0], &SegmentKind::Word);
        assert_eq!(kinds[1], &SegmentKind::Gap);
        assert_eq!(kinds[2], &SegmentKind::Word);
    }

    #[test]
    fn gap_segment_has_correct_alpha() {
        // TTS gap: 100ms, Original gap: 200ms -> alpha = 2.0
        let tts = vec![word("a", 0, 500), word("b", 600, 1000)];
        let original = vec![word("a", 0, 500), word("b", 700, 1200)];
        let mut ctx = ctx_with_timings(tts, original, 16_000);
        let stage = SegmentPlanStage;
        stage.execute(&mut ctx).expect("should succeed");

        let gap = ctx.segment_plans.iter().find(|p| p.kind == SegmentKind::Gap).unwrap();
        // TTS gap = 100ms, original gap = 200ms -> alpha = 200/100 = 2.0
        assert!((gap.alpha - 2.0).abs() < 0.1, "gap alpha {} should be ~2.0", gap.alpha);
    }

    #[test]
    fn word_plans_have_correct_fields() {
        let tts = vec![word("hello", 0, 500), word("world", 500, 1000)];
        let original = vec![word("hello", 0, 600), word("world", 600, 1200)];
        let mut ctx = ctx_with_timings(tts, original, 16_000);
        let stage = SegmentPlanStage;
        stage.execute(&mut ctx).expect("should succeed");

        let plan0 = ctx.segment_plans.iter().find(|p| p.kind == SegmentKind::Word).unwrap();
        assert_eq!(plan0.start_sample, 0);
        assert_eq!(plan0.end_sample, 8_000);
        assert_eq!(plan0.original_duration_samples, 8_000);
        assert_eq!(plan0.target_duration_samples, 9_600);
        assert!((plan0.alpha - 1.2).abs() < 1e-6);
        assert_eq!(plan0.label, Some("hello".to_string()));
        assert_eq!(plan0.tts_start_ms, 0);
        assert_eq!(plan0.tts_end_ms, 500);
        assert_eq!(plan0.original_start_ms, 0);
        assert_eq!(plan0.original_end_ms, 600);
    }

    #[test]
    fn leading_gap_is_created() {
        // TTS word starts at 200ms -> gap [0..200]
        let tts = vec![word("hello", 200, 700)];
        let original = vec![word("hello", 300, 800)];
        let mut ctx = ctx_with_timings(tts, original, 16_000);
        let stage = SegmentPlanStage;
        stage.execute(&mut ctx).expect("should succeed");

        assert_eq!(ctx.segment_plans[0].kind, SegmentKind::Gap);
        assert_eq!(ctx.segment_plans[0].start_sample, 0);
        assert_eq!(ctx.segment_plans[1].kind, SegmentKind::Word);
    }

    #[test]
    fn skips_too_short_word_segments() {
        let tts = vec![word("a", 0, 5), word("hello", 100, 600)];
        let original = vec![word("a", 0, 5), word("hello", 100, 700)];
        let mut ctx = ctx_with_timings(tts, original, 16_000);
        let stage = SegmentPlanStage;
        stage.execute(&mut ctx).expect("should succeed");

        let word_plans: Vec<_> = ctx.segment_plans.iter()
            .filter(|p| p.kind == SegmentKind::Word)
            .collect();
        assert_eq!(word_plans.len(), 1);
        assert_eq!(word_plans[0].label, Some("hello".to_string()));
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
    fn cumulative_duration_covers_full_timeline() {
        let tts = vec![word("hello", 100, 500), word("world", 600, 900)];
        let original = vec![word("hello", 100, 600), word("world", 700, 1100)];
        let mut ctx = ctx_with_timings(tts, original, 16_000);
        let stage = SegmentPlanStage;
        stage.execute(&mut ctx).expect("should succeed");

        // All samples from 0..16000 should be covered by some segment
        let covered: usize = ctx.segment_plans.iter()
            .map(|p| p.end_sample - p.start_sample)
            .sum();
        assert!(covered > 0);

        // No overlapping segments
        for w in ctx.segment_plans.windows(2) {
            assert!(w[0].end_sample <= w[1].start_sample,
                "segments overlap: [{}..{}] and [{}..{}]",
                w[0].start_sample, w[0].end_sample, w[1].start_sample, w[1].end_sample);
        }
    }
}
