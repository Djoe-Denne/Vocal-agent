use tempo_domain::{
    DomainError, SegmentSynthesisPlan, SynthesisPlacement, TempoPipelineContext,
    TempoPipelineStage,
};

/// Step 11: map each synthesis mark to a source grain.
///
/// Ensures monotone progression through the analysis grains: grain indices
/// never go backwards. Duplication (stretch) and skipping (compress) are
/// both allowed.
pub struct SynthesisMappingStage;

impl TempoPipelineStage for SynthesisMappingStage {
    fn name(&self) -> &'static str {
        "synthesis_mapping"
    }

    fn execute(&self, context: &mut TempoPipelineContext) -> Result<(), DomainError> {
        if context.synthesis_grids.is_empty() {
            return Err(DomainError::internal_error(
                "synthesis_mapping: no synthesis grids available",
            ));
        }
        if context.grains.len() != context.synthesis_grids.len() {
            return Err(DomainError::internal_error(
                "synthesis_mapping: grains and synthesis_grids count mismatch",
            ));
        }

        let mut all_plans = Vec::with_capacity(context.synthesis_grids.len());

        for (seg_idx, grid) in context.synthesis_grids.iter().enumerate() {
            let grain_count = context.grains[seg_idx].grains.len();
            if grain_count == 0 {
                all_plans.push(SegmentSynthesisPlan {
                    segment_index: seg_idx,
                    placements: Vec::new(),
                });
                continue;
            }

            let mut placements = Vec::with_capacity(grid.marks.len());
            let mut min_grain_idx = 0usize;

            for synth_mark in &grid.marks {
                let analysis_idx = synth_mark.mapped_analysis_mark_index;

                // Find the grain whose analysis_mark_index best matches,
                // constrained to never go below min_grain_idx (monotone).
                let grain_idx =
                    find_grain_for_analysis_mark(&context.grains[seg_idx].grains, analysis_idx, min_grain_idx);

                placements.push(SynthesisPlacement {
                    output_center_sample: synth_mark.output_sample_index,
                    source_grain_index: grain_idx,
                });

                min_grain_idx = grain_idx;
            }

            tracing::trace!(
                segment_index = seg_idx,
                placement_count = placements.len(),
                "synthesis mapping complete for segment"
            );

            all_plans.push(SegmentSynthesisPlan {
                segment_index: seg_idx,
                placements,
            });
        }

        tracing::debug!(
            segment_count = all_plans.len(),
            total_placements = all_plans.iter().map(|p| p.placements.len()).sum::<usize>(),
            "synthesis mapping complete"
        );

        context.synthesis_plans = all_plans;
        Ok(())
    }
}

/// Find the grain whose `analysis_mark_index` is closest to `target_analysis_idx`,
/// but never returns an index below `min_idx` (monotone constraint).
fn find_grain_for_analysis_mark(
    grains: &[tempo_domain::Grain],
    target_analysis_idx: usize,
    min_idx: usize,
) -> usize {
    if grains.is_empty() {
        return 0;
    }

    let search_start = min_idx.min(grains.len() - 1);
    let mut best_idx = search_start;
    let mut best_dist = usize::MAX;

    for i in search_start..grains.len() {
        let dist = if grains[i].analysis_mark_index >= target_analysis_idx {
            grains[i].analysis_mark_index - target_analysis_idx
        } else {
            target_analysis_idx - grains[i].analysis_mark_index
        };

        if dist < best_dist {
            best_dist = dist;
            best_idx = i;
        }

        // Once we've passed the target and distance is increasing, stop
        if grains[i].analysis_mark_index > target_analysis_idx && dist > best_dist {
            break;
        }
    }

    best_idx
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempo_domain::{
        Grain, SegmentGrains, SegmentSynthesisGrid, SynthesisMark, TempoPipelineContext,
    };

    fn make_ctx(
        grains: Vec<Grain>,
        marks: Vec<SynthesisMark>,
    ) -> TempoPipelineContext {
        let mut ctx = TempoPipelineContext::new(vec![0.0; 1600], 16_000, Vec::new(), Vec::new());
        ctx.grains = vec![SegmentGrains {
            segment_index: 0,
            grains,
        }];
        ctx.synthesis_grids = vec![SegmentSynthesisGrid {
            segment_index: 0,
            marks,
        }];
        ctx
    }

    fn grain(mark_idx: usize) -> Grain {
        Grain {
            analysis_mark_index: mark_idx,
            center_sample: mark_idx * 80,
            windowed_samples: vec![0.5; 160],
        }
    }

    fn synth_mark(out_idx: usize, analysis_idx: usize) -> SynthesisMark {
        SynthesisMark {
            output_sample_index: out_idx,
            mapped_analysis_mark_index: analysis_idx,
        }
    }

    #[test]
    fn maps_each_synth_mark_to_a_grain() {
        let grains = (0..5).map(grain).collect();
        let marks = vec![
            synth_mark(0, 0),
            synth_mark(80, 1),
            synth_mark(160, 2),
            synth_mark(240, 3),
            synth_mark(320, 4),
        ];
        let mut ctx = make_ctx(grains, marks);

        let stage = SynthesisMappingStage;
        stage.execute(&mut ctx).expect("should succeed");

        let plan = &ctx.synthesis_plans[0];
        assert_eq!(plan.placements.len(), 5);
        for (i, p) in plan.placements.iter().enumerate() {
            assert_eq!(p.source_grain_index, i);
        }
    }

    #[test]
    fn monotone_grain_progression() {
        let grains = (0..5).map(grain).collect();
        // Stretched: more synth marks than grains, duplicates expected
        let marks = vec![
            synth_mark(0, 0),
            synth_mark(50, 0),
            synth_mark(100, 1),
            synth_mark(150, 1),
            synth_mark(200, 2),
            synth_mark(250, 3),
            synth_mark(300, 3),
            synth_mark(350, 4),
        ];
        let mut ctx = make_ctx(grains, marks);

        let stage = SynthesisMappingStage;
        stage.execute(&mut ctx).expect("should succeed");

        let plan = &ctx.synthesis_plans[0];
        for w in plan.placements.windows(2) {
            assert!(
                w[1].source_grain_index >= w[0].source_grain_index,
                "grain indices should never go backwards"
            );
        }
    }

    #[test]
    fn compression_allows_grain_skipping() {
        let grains = (0..10).map(grain).collect();
        // Compressed: fewer synth marks than grains
        let marks = vec![
            synth_mark(0, 0),
            synth_mark(100, 3),
            synth_mark(200, 7),
        ];
        let mut ctx = make_ctx(grains, marks);

        let stage = SynthesisMappingStage;
        stage.execute(&mut ctx).expect("should succeed");

        let plan = &ctx.synthesis_plans[0];
        assert_eq!(plan.placements.len(), 3);
        assert!(plan.placements[1].source_grain_index > plan.placements[0].source_grain_index);
    }

    #[test]
    fn empty_grains_produces_empty_plan() {
        let marks = vec![synth_mark(0, 0)];
        let mut ctx = make_ctx(vec![], marks);

        let stage = SynthesisMappingStage;
        stage.execute(&mut ctx).expect("should succeed");

        assert!(ctx.synthesis_plans[0].placements.is_empty());
    }

    #[test]
    fn rejects_empty_grids() {
        let mut ctx = TempoPipelineContext::new(vec![0.0; 100], 16_000, Vec::new(), Vec::new());
        let stage = SynthesisMappingStage;
        assert!(stage.execute(&mut ctx).is_err());
    }
}
