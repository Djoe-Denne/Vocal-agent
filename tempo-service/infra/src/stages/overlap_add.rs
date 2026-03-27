use tempo_domain::{DomainError, SegmentKind, TempoPipelineContext, TempoPipelineStage};

const WEIGHT_FLOOR: f32 = 1e-8;

/// Step 12: overlap-add resynthesis.
///
/// Creates an output buffer for each segment, places each windowed grain at
/// its synthesis position, accumulates Hann window weights in a parallel
/// buffer, and normalizes by dividing signal by weight sum. The resynthesized
/// samples are written to `segment_audios[i].rendered_samples`.
/// Gap segments are skipped (they already have rendered_samples from extraction).
pub struct OverlapAddStage;

impl TempoPipelineStage for OverlapAddStage {
    fn name(&self) -> &'static str {
        "overlap_add"
    }

    fn execute(&self, context: &mut TempoPipelineContext) -> Result<(), DomainError> {
        if context.synthesis_plans.is_empty() {
            return Err(DomainError::internal_error(
                "overlap_add: no synthesis plans available",
            ));
        }
        if context.grains.len() != context.synthesis_plans.len() {
            return Err(DomainError::internal_error(
                "overlap_add: grains and synthesis_plans count mismatch",
            ));
        }
        if context.segment_audios.len() != context.synthesis_plans.len() {
            return Err(DomainError::internal_error(
                "overlap_add: segment_audios and synthesis_plans count mismatch",
            ));
        }

        for (seg_idx, plan) in context.synthesis_plans.iter().enumerate() {
            if context.segment_audios[seg_idx].kind == SegmentKind::Gap {
                continue;
            }

            let grains = &context.grains[seg_idx].grains;
            let target_len = context.segment_audios[seg_idx].target_duration_samples;
            let output_len = if target_len > 0 { target_len } else {
                context.segment_audios[seg_idx].analysis_samples.len()
            };

            let mut output = vec![0.0f32; output_len];
            let mut weights = vec![0.0f32; output_len];

            for placement in &plan.placements {
                if placement.source_grain_index >= grains.len() {
                    continue;
                }
                let grain = &grains[placement.source_grain_index];
                let grain_len = grain.windowed_samples.len();
                if grain_len == 0 {
                    continue;
                }

                let half = grain_len / 2;
                let out_center = placement.output_center_sample;
                let out_start = out_center.saturating_sub(half);

                let window = hann_window(grain_len);

                for (j, (sample, w)) in grain
                    .windowed_samples
                    .iter()
                    .zip(window.iter())
                    .enumerate()
                {
                    let out_idx = out_start + j;
                    if out_idx < output_len {
                        output[out_idx] += sample;
                        weights[out_idx] += w;
                    }
                }
            }

            for (sample, weight) in output.iter_mut().zip(weights.iter()) {
                if *weight > WEIGHT_FLOOR {
                    *sample /= *weight;
                }
            }

            // Fill gaps with analysis samples (mapped through useful coordinates)
            let analysis = &context.segment_audios[seg_idx].analysis_samples;
            let mut gap_fill_count = 0usize;
            for (i, weight) in weights.iter().enumerate() {
                if *weight <= WEIGHT_FLOOR && i < analysis.len() {
                    output[i] = analysis[i];
                    gap_fill_count += 1;
                }
            }

            let gap_fill_pct = if output_len > 0 {
                (gap_fill_count as f32 / output_len as f32) * 100.0
            } else {
                0.0
            };

            tracing::debug!(
                segment_index = seg_idx,
                output_len = output.len(),
                placement_count = plan.placements.len(),
                grain_count = grains.len(),
                gap_fill_samples = gap_fill_count,
                gap_fill_pct,
                "overlap-add complete for segment"
            );

            context.segment_audios[seg_idx].rendered_samples = output;
        }

        tracing::debug!(
            segment_count = context.synthesis_plans.len(),
            sample_rate_hz = context.sample_rate_hz,
            "overlap-add resynthesis complete"
        );

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
    use tempo_domain::{
        Grain, SegmentAudio, SegmentGrains, SegmentKind, SegmentSynthesisPlan,
        SynthesisPlacement, TempoPipelineContext,
    };

    fn make_ctx(
        original_len: usize,
        target_len: usize,
        grains: Vec<Grain>,
        placements: Vec<SynthesisPlacement>,
    ) -> TempoPipelineContext {
        let mut ctx = TempoPipelineContext::new(
            vec![0.1; original_len],
            16_000,
            Vec::new(),
            Vec::new(),
        );
        ctx.segment_audios = vec![SegmentAudio {
            analysis_samples: vec![0.1; original_len],
            rendered_samples: Vec::new(),
            global_start_sample: 0,
            global_end_sample: original_len,
            extract_start_sample: 0,
            extract_end_sample: original_len,
            useful_start_in_analysis: 0,
            useful_end_in_analysis: original_len,
            target_duration_samples: target_len,
            alpha: target_len as f64 / original_len as f64,
            kind: SegmentKind::Word,
        }];
        ctx.grains = vec![SegmentGrains {
            segment_index: 0,
            grains,
        }];
        ctx.synthesis_plans = vec![SegmentSynthesisPlan {
            segment_index: 0,
            placements,
        }];
        ctx
    }

    fn make_grain(mark_idx: usize, center: usize, len: usize) -> Grain {
        let w = hann_window(len);
        let windowed: Vec<f32> = w.iter().map(|x| x * 0.8).collect();
        Grain {
            analysis_mark_index: mark_idx,
            center_sample: center,
            windowed_samples: windowed,
        }
    }

    #[test]
    fn produces_output_of_target_length() {
        let grain = make_grain(0, 80, 160);
        let placement = SynthesisPlacement {
            output_center_sample: 80,
            source_grain_index: 0,
        };
        let mut ctx = make_ctx(200, 300, vec![grain], vec![placement]);

        let stage = OverlapAddStage;
        stage.execute(&mut ctx).expect("should succeed");

        assert_eq!(ctx.segment_audios[0].rendered_samples.len(), 300);
    }

    #[test]
    fn overlapping_grains_are_normalized() {
        let g1 = make_grain(0, 80, 160);
        let g2 = make_grain(1, 160, 160);
        let placements = vec![
            SynthesisPlacement {
                output_center_sample: 80,
                source_grain_index: 0,
            },
            SynthesisPlacement {
                output_center_sample: 160,
                source_grain_index: 1,
            },
        ];
        let mut ctx = make_ctx(320, 320, vec![g1, g2], placements);

        let stage = OverlapAddStage;
        stage.execute(&mut ctx).expect("should succeed");

        let output = &ctx.segment_audios[0].rendered_samples;
        for &s in &output[80..160] {
            assert!(s.is_finite());
            assert!(s.abs() < 2.0);
        }
    }

    #[test]
    fn gaps_filled_with_analysis_samples() {
        let placement = SynthesisPlacement {
            output_center_sample: 80,
            source_grain_index: 0,
        };
        let grain = make_grain(0, 80, 40);
        let mut ctx = make_ctx(200, 200, vec![grain], vec![placement]);
        for s in &mut ctx.segment_audios[0].analysis_samples {
            *s = 0.42;
        }

        let stage = OverlapAddStage;
        stage.execute(&mut ctx).expect("should succeed");

        assert!((ctx.segment_audios[0].rendered_samples[190] - 0.42).abs() < 1e-6);
    }

    #[test]
    fn empty_placements_preserve_analysis() {
        let mut ctx = make_ctx(200, 200, vec![], vec![]);

        let stage = OverlapAddStage;
        stage.execute(&mut ctx).expect("should succeed");

        for &s in &ctx.segment_audios[0].rendered_samples {
            assert!((s - 0.1).abs() < 1e-6);
        }
    }

    #[test]
    fn rejects_empty_synthesis_plans() {
        let mut ctx = TempoPipelineContext::new(vec![0.0; 100], 16_000, Vec::new(), Vec::new());
        let stage = OverlapAddStage;
        assert!(stage.execute(&mut ctx).is_err());
    }
}
