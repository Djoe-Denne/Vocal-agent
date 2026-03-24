use tempo_domain::{
    DomainError, SegmentStretchPlan, StretchMode, StretchRegion, TempoPipelineContext,
    TempoPipelineStage,
};

const PAUSE_ENERGY_THRESHOLD: f32 = 1e-5;
const STABILITY_THRESHOLD: f32 = 0.5;
const PAUSE_STRETCH_WEIGHT: f64 = 3.0;
const VOICED_STRETCH_WEIGHT: f64 = 2.0;
const KEEP_STRETCH_WEIGHT: f64 = 0.1;

/// Step 8: partition each segment into stretchable/compressible regions.
///
/// Classifies sub-intervals as `Pause`, `VoicedPsola`, or `KeepNearConstant`,
/// then distributes the segment's global alpha across regions weighted by
/// their stretchability so that stable voiced zones and pauses absorb most
/// of the time delta.
pub struct StretchRegionStage;

impl TempoPipelineStage for StretchRegionStage {
    fn name(&self) -> &'static str {
        "stretch_region"
    }

    fn execute(&self, context: &mut TempoPipelineContext) -> Result<(), DomainError> {
        if context.segment_audios.is_empty() {
            return Err(DomainError::internal_error(
                "stretch_region: no segment audios available",
            ));
        }
        if context.voiced_regions.len() != context.segment_audios.len() {
            return Err(DomainError::internal_error(
                "stretch_region: voiced_regions and segment_audios count mismatch",
            ));
        }

        let mut all_plans = Vec::with_capacity(context.segment_audios.len());

        for (seg_idx, seg_audio) in context.segment_audios.iter().enumerate() {
            let n = seg_audio.local_samples.len();
            if n == 0 {
                all_plans.push(SegmentStretchPlan {
                    segment_index: seg_idx,
                    regions: Vec::new(),
                });
                continue;
            }

            let voiced = &context.voiced_regions[seg_idx].regions;
            let frame_analysis = context.frame_analyses.get(seg_idx);

            let mut raw_regions = classify_regions(
                &seg_audio.local_samples,
                voiced,
                frame_analysis,
                n,
            );

            distribute_alpha(&mut raw_regions, seg_audio.alpha);

            tracing::trace!(
                segment_index = seg_idx,
                region_count = raw_regions.len(),
                segment_alpha = seg_audio.alpha,
                "stretch regions defined for segment"
            );

            all_plans.push(SegmentStretchPlan {
                segment_index: seg_idx,
                regions: raw_regions,
            });
        }

        tracing::debug!(
            segment_count = all_plans.len(),
            total_regions = all_plans.iter().map(|p| p.regions.len()).sum::<usize>(),
            "stretch region definition complete"
        );

        context.stretch_plans = all_plans;
        Ok(())
    }
}

/// Classify each sample range into Pause, VoicedPsola, or KeepNearConstant.
fn classify_regions(
    samples: &[f32],
    voiced_regions: &[tempo_domain::VoicedRegion],
    frame_analysis: Option<&tempo_domain::SegmentFrameAnalysis>,
    total_len: usize,
) -> Vec<StretchRegion> {
    // Build a sorted list of voiced intervals
    let mut voiced_intervals: Vec<(usize, usize, f32)> = voiced_regions
        .iter()
        .map(|r| (r.start_sample.min(total_len), r.end_sample.min(total_len), r.stability_score))
        .filter(|(s, e, _)| e > s)
        .collect();
    voiced_intervals.sort_by_key(|(s, _, _)| *s);

    let mut regions = Vec::new();
    let mut cursor = 0usize;

    for (vs, ve, stability) in &voiced_intervals {
        if cursor < *vs {
            let gap_mode = classify_gap(samples, cursor, *vs, frame_analysis);
            regions.push(StretchRegion {
                start_sample: cursor,
                end_sample: *vs,
                local_alpha: 1.0,
                mode: gap_mode,
            });
        }

        let mode = if *stability >= STABILITY_THRESHOLD {
            StretchMode::VoicedPsola
        } else {
            StretchMode::KeepNearConstant
        };

        regions.push(StretchRegion {
            start_sample: *vs,
            end_sample: *ve,
            local_alpha: 1.0,
            mode,
        });

        cursor = *ve;
    }

    if cursor < total_len {
        let gap_mode = classify_gap(samples, cursor, total_len, frame_analysis);
        regions.push(StretchRegion {
            start_sample: cursor,
            end_sample: total_len,
            local_alpha: 1.0,
            mode: gap_mode,
        });
    }

    regions
}

/// Classify a gap between voiced regions as Pause or KeepNearConstant.
fn classify_gap(
    samples: &[f32],
    start: usize,
    end: usize,
    _frame_analysis: Option<&tempo_domain::SegmentFrameAnalysis>,
) -> StretchMode {
    if start >= end || start >= samples.len() {
        return StretchMode::KeepNearConstant;
    }
    let slice_end = end.min(samples.len());
    let slice = &samples[start..slice_end];
    let rms = (slice.iter().map(|s| s * s).sum::<f32>() / slice.len().max(1) as f32).sqrt();
    if rms < PAUSE_ENERGY_THRESHOLD {
        StretchMode::Pause
    } else {
        StretchMode::KeepNearConstant
    }
}

/// Distribute the segment's global alpha across regions proportional to their
/// stretch weight. Regions with higher weight absorb more of the time delta.
fn distribute_alpha(regions: &mut [StretchRegion], global_alpha: f64) {
    if regions.is_empty() || global_alpha <= 0.0 {
        return;
    }

    let total_original: f64 = regions
        .iter()
        .map(|r| (r.end_sample - r.start_sample) as f64)
        .sum();
    if total_original <= 0.0 {
        return;
    }

    let total_target = total_original * global_alpha;
    let delta = total_target - total_original;

    let weighted_sum: f64 = regions
        .iter()
        .map(|r| {
            let len = (r.end_sample - r.start_sample) as f64;
            len * weight_for_mode(&r.mode)
        })
        .sum();

    if weighted_sum.abs() < 1e-12 {
        for r in regions.iter_mut() {
            r.local_alpha = global_alpha;
        }
        return;
    }

    for r in regions.iter_mut() {
        let len = (r.end_sample - r.start_sample) as f64;
        if len <= 0.0 {
            r.local_alpha = 1.0;
            continue;
        }
        let w = weight_for_mode(&r.mode);
        let region_delta = delta * (len * w) / weighted_sum;
        let region_target = len + region_delta;
        r.local_alpha = (region_target / len).max(0.1);
    }
}

fn weight_for_mode(mode: &StretchMode) -> f64 {
    match mode {
        StretchMode::Pause => PAUSE_STRETCH_WEIGHT,
        StretchMode::VoicedPsola => VOICED_STRETCH_WEIGHT,
        StretchMode::KeepNearConstant => KEEP_STRETCH_WEIGHT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempo_domain::{
        SegmentAudio, SegmentFrameAnalysis, SegmentVoicedRegions, TempoPipelineContext,
        VoicedRegion,
    };

    fn make_ctx(
        samples: Vec<f32>,
        alpha: f64,
        voiced: Vec<VoicedRegion>,
    ) -> TempoPipelineContext {
        let n = samples.len();
        let mut ctx = TempoPipelineContext::new(samples.clone(), 16_000, Vec::new(), Vec::new());
        ctx.segment_audios = vec![SegmentAudio {
            local_samples: samples,
            global_start_sample: 0,
            global_end_sample: n,
            margin_left: 0,
            margin_right: 0,
            target_duration_samples: (n as f64 * alpha) as usize,
            alpha,
        }];
        ctx.voiced_regions = vec![SegmentVoicedRegions {
            segment_index: 0,
            regions: voiced,
        }];
        ctx.frame_analyses = vec![SegmentFrameAnalysis {
            segment_index: 0,
            frame_length_samples: 480,
            hop_samples: 160,
            frames: Vec::new(),
        }];
        ctx
    }

    #[test]
    fn creates_stretch_regions_with_voiced_and_gaps() {
        let samples = vec![0.5; 3200]; // 200ms at 16kHz
        let voiced = vec![VoicedRegion {
            start_sample: 800,
            end_sample: 2400,
            mean_f0: 200.0,
            mean_period_samples: 80.0,
            stability_score: 0.95,
        }];
        let mut ctx = make_ctx(samples, 1.5, voiced);

        let stage = StretchRegionStage;
        stage.execute(&mut ctx).expect("should succeed");

        let plan = &ctx.stretch_plans[0];
        assert_eq!(plan.regions.len(), 3);

        assert_eq!(plan.regions[0].start_sample, 0);
        assert_eq!(plan.regions[0].end_sample, 800);

        assert_eq!(plan.regions[1].mode, StretchMode::VoicedPsola);
        assert_eq!(plan.regions[1].start_sample, 800);
        assert_eq!(plan.regions[1].end_sample, 2400);

        assert_eq!(plan.regions[2].start_sample, 2400);
        assert_eq!(plan.regions[2].end_sample, 3200);
    }

    #[test]
    fn voiced_psola_gets_higher_alpha_than_keep() {
        let samples = vec![0.5; 3200];
        let voiced = vec![VoicedRegion {
            start_sample: 800,
            end_sample: 2400,
            mean_f0: 200.0,
            mean_period_samples: 80.0,
            stability_score: 0.95,
        }];
        let mut ctx = make_ctx(samples, 1.5, voiced);

        let stage = StretchRegionStage;
        stage.execute(&mut ctx).expect("should succeed");

        let plan = &ctx.stretch_plans[0];
        let voiced_alpha = plan.regions[1].local_alpha;
        let keep_alpha = plan.regions[0].local_alpha;
        assert!(
            voiced_alpha > keep_alpha,
            "voiced alpha {voiced_alpha} should exceed keep alpha {keep_alpha}"
        );
    }

    #[test]
    fn silent_gap_classified_as_pause() {
        let mut samples = vec![0.0; 1600]; // silent gap
        samples.extend(vec![0.5; 1600]); // voiced content
        let voiced = vec![VoicedRegion {
            start_sample: 1600,
            end_sample: 3200,
            mean_f0: 200.0,
            mean_period_samples: 80.0,
            stability_score: 0.9,
        }];
        let mut ctx = make_ctx(samples, 1.2, voiced);

        let stage = StretchRegionStage;
        stage.execute(&mut ctx).expect("should succeed");

        assert_eq!(ctx.stretch_plans[0].regions[0].mode, StretchMode::Pause);
    }

    #[test]
    fn low_stability_region_becomes_keep_near_constant() {
        let samples = vec![0.5; 3200];
        let voiced = vec![VoicedRegion {
            start_sample: 0,
            end_sample: 3200,
            mean_f0: 200.0,
            mean_period_samples: 80.0,
            stability_score: 0.2,
        }];
        let mut ctx = make_ctx(samples, 1.3, voiced);

        let stage = StretchRegionStage;
        stage.execute(&mut ctx).expect("should succeed");

        assert_eq!(
            ctx.stretch_plans[0].regions[0].mode,
            StretchMode::KeepNearConstant
        );
    }

    #[test]
    fn alpha_sums_preserve_total_target_duration() {
        let samples = vec![0.5; 4800];
        let voiced = vec![VoicedRegion {
            start_sample: 1600,
            end_sample: 3200,
            mean_f0: 200.0,
            mean_period_samples: 80.0,
            stability_score: 0.95,
        }];
        let global_alpha = 1.4;
        let mut ctx = make_ctx(samples, global_alpha, voiced);

        let stage = StretchRegionStage;
        stage.execute(&mut ctx).expect("should succeed");

        let total_target: f64 = ctx.stretch_plans[0]
            .regions
            .iter()
            .map(|r| (r.end_sample - r.start_sample) as f64 * r.local_alpha)
            .sum();
        let expected = 4800.0 * global_alpha;
        assert!(
            (total_target - expected).abs() < 1.0,
            "total target {total_target} should match expected {expected}"
        );
    }

    #[test]
    fn rejects_empty_segment_audios() {
        let mut ctx = TempoPipelineContext::new(vec![0.0; 100], 16_000, Vec::new(), Vec::new());
        let stage = StretchRegionStage;
        assert!(stage.execute(&mut ctx).is_err());
    }
}
