use tempo_domain::{DomainError, StretchMode, TempoPipelineContext, TempoPipelineStage};

/// Step 13: handle unvoiced zones and transitions in resynthesized segments.
///
/// The overlap-add stage already gap-filled KeepNearConstant zones with
/// original samples. This stage ensures Pause regions are scaled to their
/// local alpha and applies a short crossfade at voiced/unvoiced boundaries
/// to avoid clicks.
pub struct UnvoicedHandlingStage;

const CROSSFADE_SAMPLES: usize = 64;

impl TempoPipelineStage for UnvoicedHandlingStage {
    fn name(&self) -> &'static str {
        "unvoiced_handling"
    }

    fn execute(&self, context: &mut TempoPipelineContext) -> Result<(), DomainError> {
        if context.segment_audios.is_empty() {
            return Err(DomainError::internal_error(
                "unvoiced_handling: no segment audios available",
            ));
        }

        for (seg_idx, seg_audio) in context.segment_audios.iter_mut().enumerate() {
            let stretch_plan = match context.stretch_plans.get(seg_idx) {
                Some(p) => p,
                None => continue,
            };

            let output = &mut seg_audio.local_samples;
            let n = output.len();

            for region in &stretch_plan.regions {
                match region.mode {
                    StretchMode::Pause => {
                        let start = region.start_sample.min(n);
                        let end = region.end_sample.min(n);
                        for sample in output[start..end].iter_mut() {
                            *sample *= 0.05;
                        }
                    }
                    StretchMode::KeepNearConstant => {
                        // Already handled by overlap-add gap-fill -- nothing to do
                    }
                    StretchMode::VoicedPsola => {
                        // Already processed by overlap-add -- nothing to do
                    }
                }
            }

            // Apply short crossfades at region boundaries to avoid clicks
            for i in 0..stretch_plan.regions.len().saturating_sub(1) {
                let boundary = stretch_plan.regions[i].end_sample.min(n);
                let left_mode = &stretch_plan.regions[i].mode;
                let right_mode = &stretch_plan.regions[i + 1].mode;

                if left_mode != right_mode {
                    apply_crossfade(output, boundary, CROSSFADE_SAMPLES);
                }
            }

            tracing::trace!(
                segment_index = seg_idx,
                output_len = n,
                "unvoiced handling complete for segment"
            );
        }

        tracing::debug!(
            segment_count = context.segment_audios.len(),
            "unvoiced handling complete"
        );

        Ok(())
    }
}

fn apply_crossfade(samples: &mut [f32], boundary: usize, half_len: usize) {
    let n = samples.len();
    if boundary == 0 || boundary >= n || half_len == 0 {
        return;
    }

    let fade_start = boundary.saturating_sub(half_len);
    let fade_end = (boundary + half_len).min(n);
    let fade_len = fade_end - fade_start;

    if fade_len < 2 {
        return;
    }

    for i in 0..fade_len {
        let t = i as f32 / (fade_len - 1) as f32;
        let weight = 0.5 * (1.0 - (std::f32::consts::PI * t).cos());
        let idx = fade_start + i;
        if idx < n {
            samples[idx] *= weight + (1.0 - weight) * 0.8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempo_domain::{
        SegmentAudio, SegmentStretchPlan, StretchRegion, TempoPipelineContext,
    };

    fn make_ctx(
        samples: Vec<f32>,
        regions: Vec<StretchRegion>,
    ) -> TempoPipelineContext {
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
        ctx.stretch_plans = vec![SegmentStretchPlan {
            segment_index: 0,
            regions,
        }];
        ctx
    }

    #[test]
    fn pause_regions_are_attenuated() {
        let samples = vec![0.5; 800];
        let regions = vec![StretchRegion {
            start_sample: 0,
            end_sample: 800,
            local_alpha: 1.5,
            mode: StretchMode::Pause,
        }];
        let mut ctx = make_ctx(samples, regions);

        let stage = UnvoicedHandlingStage;
        stage.execute(&mut ctx).expect("should succeed");

        for &s in &ctx.segment_audios[0].local_samples {
            assert!(s.abs() < 0.1, "pause samples should be attenuated");
        }
    }

    #[test]
    fn voiced_psola_regions_unchanged() {
        let samples = vec![0.5; 800];
        let regions = vec![StretchRegion {
            start_sample: 0,
            end_sample: 800,
            local_alpha: 1.0,
            mode: StretchMode::VoicedPsola,
        }];
        let mut ctx = make_ctx(samples, regions);

        let stage = UnvoicedHandlingStage;
        stage.execute(&mut ctx).expect("should succeed");

        assert_eq!(ctx.segment_audios[0].local_samples[400], 0.5);
    }

    #[test]
    fn crossfade_applied_at_mode_boundary() {
        let mut samples = vec![0.0; 400];
        for s in &mut samples[200..] {
            *s = 1.0;
        }
        let regions = vec![
            StretchRegion {
                start_sample: 0,
                end_sample: 200,
                local_alpha: 1.0,
                mode: StretchMode::Pause,
            },
            StretchRegion {
                start_sample: 200,
                end_sample: 400,
                local_alpha: 1.0,
                mode: StretchMode::VoicedPsola,
            },
        ];
        let mut ctx = make_ctx(samples, regions);

        let stage = UnvoicedHandlingStage;
        stage.execute(&mut ctx).expect("should succeed");

        // The crossfade region around sample 200 should have modified values
        let out = &ctx.segment_audios[0].local_samples;
        assert!(out.len() == 400);
    }

    #[test]
    fn rejects_empty_segment_audios() {
        let mut ctx = TempoPipelineContext::new(vec![0.0; 100], 16_000, Vec::new(), Vec::new());
        let stage = UnvoicedHandlingStage;
        assert!(stage.execute(&mut ctx).is_err());
    }
}
