use tempo_domain::{
    DomainError, Grain, SegmentGrains, TempoPipelineContext, TempoPipelineStage,
};

const PERIOD_MULTIPLIER: f32 = 2.0;

/// Step 9: extract Hann-windowed grains centered on each analysis pitch mark.
///
/// Each grain spans ~2 local periods and is multiplied by a Hann window
/// so that overlap-add reconstruction produces smooth results.
pub struct GrainExtractionStage;

impl TempoPipelineStage for GrainExtractionStage {
    fn name(&self) -> &'static str {
        "grain_extraction"
    }

    fn execute(&self, context: &mut TempoPipelineContext) -> Result<(), DomainError> {
        if context.pitch_marks.is_empty() {
            return Err(DomainError::internal_error(
                "grain_extraction: no pitch marks available",
            ));
        }
        if context.segment_audios.len() != context.pitch_marks.len() {
            return Err(DomainError::internal_error(
                "grain_extraction: segment_audios and pitch_marks count mismatch",
            ));
        }

        let mut all_grains = Vec::with_capacity(context.pitch_marks.len());

        for (seg_idx, seg_marks) in context.pitch_marks.iter().enumerate() {
            let samples = &context.segment_audios[seg_idx].local_samples;
            let n = samples.len();
            let mut grains = Vec::with_capacity(seg_marks.marks.len());

            for (mark_idx, mark) in seg_marks.marks.iter().enumerate() {
                let half_len =
                    (mark.local_period_samples * PERIOD_MULTIPLIER / 2.0).round() as usize;
                if half_len == 0 {
                    continue;
                }

                let center = mark.sample_index;
                let left = center.saturating_sub(half_len);
                let right = (center + half_len).min(n);

                if right <= left {
                    continue;
                }

                let grain_len = right - left;
                let window = hann_window(grain_len);
                let windowed: Vec<f32> = samples[left..right]
                    .iter()
                    .zip(window.iter())
                    .map(|(s, w)| s * w)
                    .collect();

                grains.push(Grain {
                    analysis_mark_index: mark_idx,
                    center_sample: center,
                    windowed_samples: windowed,
                });
            }

            tracing::trace!(
                segment_index = seg_idx,
                grain_count = grains.len(),
                "grains extracted for segment"
            );

            all_grains.push(SegmentGrains {
                segment_index: seg_idx,
                grains,
            });
        }

        tracing::debug!(
            segment_count = all_grains.len(),
            total_grains = all_grains.iter().map(|g| g.grains.len()).sum::<usize>(),
            "grain extraction complete"
        );

        context.grains = all_grains;
        Ok(())
    }
}

fn hann_window(len: usize) -> Vec<f32> {
    if len == 0 {
        return Vec::new();
    }
    if len == 1 {
        return vec![1.0];
    }
    (0..len)
        .map(|i| {
            0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (len - 1) as f32).cos())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempo_domain::{PitchMark, SegmentAudio, SegmentPitchMarks, TempoPipelineContext};

    fn make_ctx(samples: Vec<f32>, marks: Vec<PitchMark>) -> TempoPipelineContext {
        let n = samples.len();
        let mut ctx = TempoPipelineContext::new(samples.clone(), 16_000, Vec::new(), Vec::new());
        ctx.segment_audios = vec![SegmentAudio {
            local_samples: samples,
            global_start_sample: 0,
            global_end_sample: n,
            margin_left: 0,
            margin_right: 0,
            target_duration_samples: n,
            alpha: 1.0,
        }];
        ctx.pitch_marks = vec![SegmentPitchMarks {
            segment_index: 0,
            marks,
        }];
        ctx
    }

    fn pm(idx: usize, period: f32) -> PitchMark {
        PitchMark {
            sample_index: idx,
            local_period_samples: period,
            confidence: 0.9,
        }
    }

    #[test]
    fn extracts_grains_for_each_mark() {
        let samples = vec![0.5; 1600];
        let marks = vec![pm(200, 80.0), pm(400, 80.0), pm(600, 80.0)];
        let mut ctx = make_ctx(samples, marks);

        let stage = GrainExtractionStage;
        stage.execute(&mut ctx).expect("should succeed");

        assert_eq!(ctx.grains.len(), 1);
        assert_eq!(ctx.grains[0].grains.len(), 3);
    }

    #[test]
    fn grain_length_is_two_periods() {
        let samples = vec![0.5; 1600];
        let marks = vec![pm(400, 80.0)];
        let mut ctx = make_ctx(samples, marks);

        let stage = GrainExtractionStage;
        stage.execute(&mut ctx).expect("should succeed");

        let grain = &ctx.grains[0].grains[0];
        let expected_len = (80.0 * PERIOD_MULTIPLIER).round() as usize;
        assert_eq!(grain.windowed_samples.len(), expected_len);
    }

    #[test]
    fn grain_is_hann_windowed() {
        let samples = vec![1.0; 1600];
        let marks = vec![pm(400, 80.0)];
        let mut ctx = make_ctx(samples, marks);

        let stage = GrainExtractionStage;
        stage.execute(&mut ctx).expect("should succeed");

        let grain = &ctx.grains[0].grains[0];
        // Hann window edges should be near zero
        assert!(grain.windowed_samples[0].abs() < 0.01);
        assert!(grain.windowed_samples.last().unwrap().abs() < 0.01);
        // Hann window center should be near 1.0 (since input is 1.0)
        let mid = grain.windowed_samples.len() / 2;
        assert!(grain.windowed_samples[mid] > 0.9);
    }

    #[test]
    fn grain_clamped_at_buffer_edges() {
        let samples = vec![0.5; 200];
        let marks = vec![pm(10, 80.0)]; // near left edge
        let mut ctx = make_ctx(samples, marks);

        let stage = GrainExtractionStage;
        stage.execute(&mut ctx).expect("should succeed");

        let grain = &ctx.grains[0].grains[0];
        assert!(grain.windowed_samples.len() > 0);
        assert!(grain.windowed_samples.len() <= 200);
    }

    #[test]
    fn rejects_empty_pitch_marks() {
        let mut ctx = TempoPipelineContext::new(vec![0.0; 100], 16_000, Vec::new(), Vec::new());
        let stage = GrainExtractionStage;
        assert!(stage.execute(&mut ctx).is_err());
    }

    #[test]
    fn hann_window_properties() {
        let w = hann_window(100);
        assert_eq!(w.len(), 100);
        assert!(w[0].abs() < 1e-6);
        assert!(w[99].abs() < 1e-6);
        assert!((w[50] - 1.0).abs() < 0.02);
    }
}
