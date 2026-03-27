use tempo_domain::{
    DomainError, SegmentKind, SegmentSynthesisGrid, StretchMode, SynthesisMark,
    TempoPipelineContext, TempoPipelineStage,
};

/// Step 10: build output-side synthesis marks from the stretch plan.
///
/// For VoicedPsola regions, output marks are spaced by the **source pitch
/// period** (T0) -- NOT divided by alpha. Time-stretching is achieved by the
/// input-to-output position mapping, which naturally duplicates or skips
/// analysis marks. This preserves pitch while changing duration.
/// For Pause regions, marks are proportionally mapped.
/// For KeepNearConstant regions, analysis marks are proportionally mapped.
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
            if context.segment_audios[seg_idx].kind == SegmentKind::Gap {
                all_grids.push(SegmentSynthesisGrid {
                    segment_index: seg_idx,
                    marks: Vec::new(),
                });
                continue;
            }

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

                        let region_output_len = (region.end_sample - region.start_sample) as f64
                            * region.local_alpha;
                        let region_output_end = output_cursor + region_output_len;

                        let mut pos = output_cursor;
                        while pos < region_output_end {
                            let out_idx = pos.round() as usize;
                            let input_pos = region.start_sample as f64
                                + (pos - output_cursor) / region.local_alpha.max(0.01);
                            let nearest = nearest_mark_index(analysis_marks, input_pos);

                            let local_t0 = analysis_marks.get(nearest)
                                .map(|m| m.local_period_samples as f64)
                                .unwrap_or(mean_period as f64)
                                .max(1.0);

                            output_marks.push(SynthesisMark {
                                output_sample_index: out_idx,
                                mapped_analysis_mark_index: nearest,
                            });

                            pos += local_t0;
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
        PitchMark, SegmentAudio, SegmentKind, SegmentPitchMarks, SegmentStretchPlan,
        StretchRegion, TempoPipelineContext,
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
            analysis_samples: vec![0.0; n],
            rendered_samples: Vec::new(),
            global_start_sample: 0,
            global_end_sample: n,
            extract_start_sample: 0,
            extract_end_sample: n,
            useful_start_in_analysis: 0,
            useful_end_in_analysis: n,
            target_duration_samples: target,
            alpha,
            kind: SegmentKind::Word,
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
        // Output region = 800*1.5 = 1200 samples, T0=80 -> ~15 marks
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
        // Output region = 1600*0.6 = 960 samples, T0=80 -> ~12 marks < 20
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
    fn pitch_preserved_when_stretching() {
        let period = 80.0f32;
        let alpha = 1.25;
        let marks: Vec<PitchMark> = (0..10).map(|i| pm(i * 80, period)).collect();
        let regions = vec![StretchRegion {
            start_sample: 0,
            end_sample: 800,
            local_alpha: alpha,
            mode: StretchMode::VoicedPsola,
        }];
        let mut ctx = make_ctx(800, alpha, marks, regions);

        let stage = SynthesisGridStage;
        stage.execute(&mut ctx).expect("should succeed");

        let grid = &ctx.synthesis_grids[0];
        assert!(grid.marks.len() >= 2, "need at least 2 marks for spacing check");

        let spacings: Vec<usize> = grid.marks.windows(2)
            .map(|w| w[1].output_sample_index - w[0].output_sample_index)
            .collect();
        let avg_spacing = spacings.iter().sum::<usize>() as f32 / spacings.len() as f32;

        // Output mark spacing should be ~T0 (80), NOT T0/alpha (64)
        assert!(
            (avg_spacing - period).abs() < period * 0.15,
            "avg spacing {} should be near source period {} (not {})",
            avg_spacing, period, period / alpha as f32
        );
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
