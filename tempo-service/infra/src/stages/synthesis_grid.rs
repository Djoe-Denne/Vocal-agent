use tempo_domain::{
    DomainError, SegmentSynthesisGrid, StretchMode, SynthesisMark, TempoPipelineContext,
    TempoPipelineStage,
};

/// Step 10: build output-side pitch marks from the stretch plan.
///
/// For VoicedPsola regions, output marks are spaced by `period / local_alpha`.
/// For Pause regions, mark spacing is scaled by `local_alpha`.
/// For KeepNearConstant regions, analysis marks are copied directly.
/// All marks reference back to the nearest analysis mark index.
pub struct SynthesisGridStage;

impl TempoPipelineStage for SynthesisGridStage {
    fn name(&self) -> &'static str {
        "synthesis_grid"
    }

    fn execute(&self, context: &mut TempoPipelineContext) -> Result<(), DomainError> {
        if context.stretch_plans.is_empty() {
            return Err(DomainError::internal_error(
                "synthesis_grid: no stretch plans available",
            ));
        }
        if context.pitch_marks.len() != context.stretch_plans.len() {
            return Err(DomainError::internal_error(
                "synthesis_grid: pitch_marks and stretch_plans count mismatch",
            ));
        }

        let mut all_grids = Vec::with_capacity(context.stretch_plans.len());

        for (seg_idx, stretch_plan) in context.stretch_plans.iter().enumerate() {
            let analysis_marks = &context.pitch_marks[seg_idx].marks;
            let target_len = context.segment_audios[seg_idx].target_duration_samples;

            let mut output_marks: Vec<SynthesisMark> = Vec::new();
            let mut output_cursor = 0.0f64;

            for region in &stretch_plan.regions {
                let region_analysis: Vec<(usize, &tempo_domain::PitchMark)> = analysis_marks
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| {
                        m.sample_index >= region.start_sample
                            && m.sample_index < region.end_sample
                    })
                    .collect();

                match region.mode {
                    StretchMode::VoicedPsola => {
                        if region_analysis.is_empty() {
                            let region_len =
                                (region.end_sample - region.start_sample) as f64;
                            output_cursor += region_len * region.local_alpha;
                            continue;
                        }

                        let mean_period: f32 = region_analysis
                            .iter()
                            .map(|(_, m)| m.local_period_samples)
                            .sum::<f32>()
                            / region_analysis.len() as f32;

                        let synth_period =
                            (mean_period as f64 / region.local_alpha.max(0.01)).max(1.0);

                        let region_output_len = (region.end_sample - region.start_sample) as f64
                            * region.local_alpha;
                        let region_output_end = output_cursor + region_output_len;

                        let mut pos = output_cursor;
                        while pos < region_output_end {
                            let out_idx = pos.round() as usize;
                            let input_pos = region.start_sample as f64
                                + (pos - output_cursor) / region.local_alpha.max(0.01);
                            let nearest = nearest_mark_index(analysis_marks, input_pos);
                            output_marks.push(SynthesisMark {
                                output_sample_index: out_idx,
                                mapped_analysis_mark_index: nearest,
                            });
                            pos += synth_period;
                        }

                        output_cursor = region_output_end;
                    }

                    StretchMode::Pause => {
                        let region_len = (region.end_sample - region.start_sample) as f64;
                        let output_len = region_len * region.local_alpha;

                        if !region_analysis.is_empty() {
                            for (orig_idx, mark) in &region_analysis {
                                let frac = (mark.sample_index - region.start_sample) as f64
                                    / region_len.max(1.0);
                                let out_idx = (output_cursor + frac * output_len).round() as usize;
                                output_marks.push(SynthesisMark {
                                    output_sample_index: out_idx,
                                    mapped_analysis_mark_index: *orig_idx,
                                });
                            }
                        }

                        output_cursor += output_len;
                    }

                    StretchMode::KeepNearConstant => {
                        let region_len = (region.end_sample - region.start_sample) as f64;
                        let output_len = region_len * region.local_alpha;

                        for (orig_idx, mark) in &region_analysis {
                            let frac = (mark.sample_index - region.start_sample) as f64
                                / region_len.max(1.0);
                            let out_idx = (output_cursor + frac * output_len).round() as usize;
                            output_marks.push(SynthesisMark {
                                output_sample_index: out_idx,
                                mapped_analysis_mark_index: *orig_idx,
                            });
                        }

                        output_cursor += output_len;
                    }
                }
            }

            // Clamp to target length
            for m in &mut output_marks {
                if m.output_sample_index >= target_len && target_len > 0 {
                    m.output_sample_index = target_len - 1;
                }
            }

            output_marks.sort_by_key(|m| m.output_sample_index);
            output_marks.dedup_by_key(|m| m.output_sample_index);

            tracing::trace!(
                segment_index = seg_idx,
                mark_count = output_marks.len(),
                target_len,
                "synthesis grid built for segment"
            );

            all_grids.push(SegmentSynthesisGrid {
                segment_index: seg_idx,
                marks: output_marks,
            });
        }

        tracing::debug!(
            segment_count = all_grids.len(),
            total_marks = all_grids.iter().map(|g| g.marks.len()).sum::<usize>(),
            "synthesis grid construction complete"
        );

        context.synthesis_grids = all_grids;
        Ok(())
    }
}

fn nearest_mark_index(marks: &[tempo_domain::PitchMark], target_pos: f64) -> usize {
    if marks.is_empty() {
        return 0;
    }
    let mut best_idx = 0;
    let mut best_dist = f64::MAX;
    for (i, m) in marks.iter().enumerate() {
        let dist = (m.sample_index as f64 - target_pos).abs();
        if dist < best_dist {
            best_dist = dist;
            best_idx = i;
        }
    }
    best_idx
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempo_domain::{
        PitchMark, SegmentAudio, SegmentPitchMarks, SegmentStretchPlan, StretchRegion,
        TempoPipelineContext,
    };

    fn pm(idx: usize, period: f32) -> PitchMark {
        PitchMark {
            sample_index: idx,
            local_period_samples: period,
            confidence: 0.9,
        }
    }

    fn make_ctx(
        n: usize,
        alpha: f64,
        marks: Vec<PitchMark>,
        regions: Vec<StretchRegion>,
    ) -> TempoPipelineContext {
        let target = (n as f64 * alpha) as usize;
        let mut ctx =
            TempoPipelineContext::new(vec![0.0; n], 16_000, Vec::new(), Vec::new());
        ctx.segment_audios = vec![SegmentAudio {
            local_samples: vec![0.0; n],
            global_start_sample: 0,
            global_end_sample: n,
            margin_left: 0,
            margin_right: 0,
            target_duration_samples: target,
            alpha,
        }];
        ctx.pitch_marks = vec![SegmentPitchMarks {
            segment_index: 0,
            marks,
        }];
        ctx.stretch_plans = vec![SegmentStretchPlan {
            segment_index: 0,
            regions,
        }];
        ctx
    }

    #[test]
    fn stretching_produces_more_output_marks() {
        let marks: Vec<PitchMark> = (0..10).map(|i| pm(i * 80, 80.0)).collect();
        let regions = vec![StretchRegion {
            start_sample: 0,
            end_sample: 800,
            local_alpha: 1.5,
            mode: StretchMode::VoicedPsola,
        }];
        let mut ctx = make_ctx(800, 1.5, marks, regions);

        let stage = SynthesisGridStage;
        stage.execute(&mut ctx).expect("should succeed");

        let grid = &ctx.synthesis_grids[0];
        // With alpha=1.5, output should have more marks than input (10)
        assert!(
            grid.marks.len() > 10,
            "expected more than 10 output marks, got {}",
            grid.marks.len()
        );
    }

    #[test]
    fn compressing_produces_fewer_output_marks() {
        let marks: Vec<PitchMark> = (0..20).map(|i| pm(i * 80, 80.0)).collect();
        let regions = vec![StretchRegion {
            start_sample: 0,
            end_sample: 1600,
            local_alpha: 0.6,
            mode: StretchMode::VoicedPsola,
        }];
        let mut ctx = make_ctx(1600, 0.6, marks, regions);

        let stage = SynthesisGridStage;
        stage.execute(&mut ctx).expect("should succeed");

        let grid = &ctx.synthesis_grids[0];
        assert!(
            grid.marks.len() < 20,
            "expected fewer than 20 output marks, got {}",
            grid.marks.len()
        );
    }

    #[test]
    fn output_marks_are_monotonically_increasing() {
        let marks: Vec<PitchMark> = (0..10).map(|i| pm(i * 80, 80.0)).collect();
        let regions = vec![StretchRegion {
            start_sample: 0,
            end_sample: 800,
            local_alpha: 1.3,
            mode: StretchMode::VoicedPsola,
        }];
        let mut ctx = make_ctx(800, 1.3, marks, regions);

        let stage = SynthesisGridStage;
        stage.execute(&mut ctx).expect("should succeed");

        let grid = &ctx.synthesis_grids[0];
        for w in grid.marks.windows(2) {
            assert!(
                w[0].output_sample_index < w[1].output_sample_index,
                "marks not monotonic: {} >= {}",
                w[0].output_sample_index,
                w[1].output_sample_index
            );
        }
    }

    #[test]
    fn keep_near_constant_preserves_proportions() {
        let marks = vec![pm(100, 80.0), pm(200, 80.0), pm(300, 80.0)];
        let regions = vec![StretchRegion {
            start_sample: 0,
            end_sample: 400,
            local_alpha: 1.0,
            mode: StretchMode::KeepNearConstant,
        }];
        let mut ctx = make_ctx(400, 1.0, marks, regions);

        let stage = SynthesisGridStage;
        stage.execute(&mut ctx).expect("should succeed");

        assert_eq!(ctx.synthesis_grids[0].marks.len(), 3);
    }

    #[test]
    fn rejects_empty_stretch_plans() {
        let mut ctx = TempoPipelineContext::new(vec![0.0; 100], 16_000, Vec::new(), Vec::new());
        let stage = SynthesisGridStage;
        assert!(stage.execute(&mut ctx).is_err());
    }
}
