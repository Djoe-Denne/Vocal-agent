use tempo_domain::{
    DomainError, SegmentAudio, SegmentKind, TempoPipelineContext, TempoPipelineStage,
};

const DEFAULT_MARGIN_MS: u64 = 10;
const GAP_SILENCE_RMS_THRESHOLD: f32 = 1e-4;

/// Step 3: extract per-segment audio buffers from the global signal.
///
/// For each `SegmentPlan`, copies the relevant samples into an analysis buffer
/// with a small margin on each side.  For `Gap` segments, pre-fills
/// `rendered_samples` with silence or a copy of the source audio.
pub struct SegmentExtractionStage;

impl TempoPipelineStage for SegmentExtractionStage {
    fn name(&self) -> &'static str {
        "segment_extraction"
    }

    fn execute(&self, context: &mut TempoPipelineContext) -> Result<(), DomainError> {
        if context.segment_plans.is_empty() {
            return Err(DomainError::internal_error(
                "segment_extraction: no segment plans available",
            ));
        }

        let total_samples = context.samples.len();
        let rate = context.sample_rate_hz;
        let margin_samples = tempo_domain::convert::ms_to_samples(DEFAULT_MARGIN_MS, rate);

        let mut audios = Vec::with_capacity(context.segment_plans.len());

        for plan in &context.segment_plans {
            let margin_left = plan.start_sample.min(margin_samples);
            let margin_right = (total_samples - plan.end_sample).min(margin_samples);

            let extract_start = plan.start_sample - margin_left;
            let extract_end = plan.end_sample + margin_right;

            let analysis_samples = context.samples[extract_start..extract_end].to_vec();

            let useful_start = margin_left;
            let useful_end = analysis_samples.len() - margin_right;

            let rendered_samples = if plan.kind == SegmentKind::Gap {
                let useful_slice = &analysis_samples[useful_start..useful_end];
                let rms = if useful_slice.is_empty() {
                    0.0
                } else {
                    (useful_slice.iter().map(|s| s * s).sum::<f32>()
                        / useful_slice.len() as f32)
                        .sqrt()
                };

                if rms < GAP_SILENCE_RMS_THRESHOLD {
                    vec![0.0f32; plan.target_duration_samples]
                } else {
                    // Simple linear resample for non-silent gaps
                    resample_linear(useful_slice, plan.target_duration_samples)
                }
            } else {
                Vec::new()
            };

            audios.push(SegmentAudio {
                analysis_samples,
                rendered_samples,
                global_start_sample: plan.start_sample,
                global_end_sample: plan.end_sample,
                extract_start_sample: extract_start,
                extract_end_sample: extract_end,
                useful_start_in_analysis: useful_start,
                useful_end_in_analysis: useful_end,
                target_duration_samples: plan.target_duration_samples,
                alpha: plan.alpha,
                kind: plan.kind.clone(),
            });
        }

        tracing::debug!(
            segment_count = audios.len(),
            margin_ms = DEFAULT_MARGIN_MS,
            "segment audio buffers extracted"
        );

        context.segment_audios = audios;
        Ok(())
    }
}

/// Naive linear interpolation resample from `src` to a buffer of `target_len`.
fn resample_linear(src: &[f32], target_len: usize) -> Vec<f32> {
    if target_len == 0 || src.is_empty() {
        return vec![0.0; target_len];
    }
    if src.len() == 1 {
        return vec![src[0]; target_len];
    }

    let ratio = (src.len() - 1) as f64 / (target_len - 1).max(1) as f64;
    (0..target_len)
        .map(|i| {
            let pos = i as f64 * ratio;
            let idx = pos.floor() as usize;
            let frac = pos - idx as f64;
            let a = src[idx.min(src.len() - 1)];
            let b = src[(idx + 1).min(src.len() - 1)];
            a + (b - a) * frac as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempo_domain::{SegmentKind, SegmentPlan, TempoPipelineContext};

    fn ctx_with_plans(
        samples: Vec<f32>,
        plans: Vec<SegmentPlan>,
    ) -> TempoPipelineContext {
        let mut ctx = TempoPipelineContext::new(samples, 16_000, Vec::new(), Vec::new());
        ctx.segment_plans = plans;
        ctx
    }

    fn word_plan(start: usize, end: usize) -> SegmentPlan {
        let dur = end - start;
        SegmentPlan {
            kind: SegmentKind::Word,
            start_sample: start,
            end_sample: end,
            original_duration_samples: dur,
            target_duration_samples: dur,
            alpha: 1.0,
            tts_start_ms: 0,
            tts_end_ms: 0,
            original_start_ms: 0,
            original_end_ms: 0,
            label: Some("test".into()),
        }
    }

    fn gap_plan(start: usize, end: usize, target: usize) -> SegmentPlan {
        let dur = end - start;
        SegmentPlan {
            kind: SegmentKind::Gap,
            start_sample: start,
            end_sample: end,
            original_duration_samples: dur,
            target_duration_samples: target,
            alpha: if dur > 0 { target as f64 / dur as f64 } else { 1.0 },
            tts_start_ms: 0,
            tts_end_ms: 0,
            original_start_ms: 0,
            original_end_ms: 0,
            label: None,
        }
    }

    #[test]
    fn extracts_word_segment_with_margins() {
        let samples: Vec<f32> = (0..1600).map(|i| i as f32 / 1600.0).collect();
        let mut ctx = ctx_with_plans(samples, vec![word_plan(200, 1400)]);
        let stage = SegmentExtractionStage;
        stage.execute(&mut ctx).expect("should succeed");

        assert_eq!(ctx.segment_audios.len(), 1);
        let seg = &ctx.segment_audios[0];
        assert_eq!(seg.global_start_sample, 200);
        assert_eq!(seg.global_end_sample, 1400);
        assert!(seg.useful_start_in_analysis > 0);
        let useful_len = seg.useful_end_in_analysis - seg.useful_start_in_analysis;
        assert_eq!(useful_len, 1200);
        assert!(seg.rendered_samples.is_empty(), "Word segments get empty rendered_samples");
    }

    #[test]
    fn silent_gap_gets_zero_filled_rendered() {
        let samples = vec![0.0; 1600];
        let mut ctx = ctx_with_plans(samples, vec![gap_plan(200, 800, 1200)]);
        let stage = SegmentExtractionStage;
        stage.execute(&mut ctx).expect("should succeed");

        let seg = &ctx.segment_audios[0];
        assert_eq!(seg.kind, SegmentKind::Gap);
        assert_eq!(seg.rendered_samples.len(), 1200);
        assert!(seg.rendered_samples.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn noisy_gap_gets_resampled_rendered() {
        let samples = vec![0.5; 1600];
        let mut ctx = ctx_with_plans(samples, vec![gap_plan(200, 800, 1200)]);
        let stage = SegmentExtractionStage;
        stage.execute(&mut ctx).expect("should succeed");

        let seg = &ctx.segment_audios[0];
        assert_eq!(seg.rendered_samples.len(), 1200);
        assert!(seg.rendered_samples.iter().any(|&s| s > 0.0));
    }

    #[test]
    fn margin_clamped_at_buffer_start() {
        let samples = vec![0.0; 1600];
        let mut ctx = ctx_with_plans(samples, vec![word_plan(0, 800)]);
        let stage = SegmentExtractionStage;
        stage.execute(&mut ctx).expect("should succeed");

        let seg = &ctx.segment_audios[0];
        assert_eq!(seg.useful_start_in_analysis, 0);
        assert_eq!(seg.extract_start_sample, 0);
    }

    #[test]
    fn margin_clamped_at_buffer_end() {
        let samples = vec![0.0; 1600];
        let mut ctx = ctx_with_plans(samples, vec![word_plan(800, 1600)]);
        let stage = SegmentExtractionStage;
        stage.execute(&mut ctx).expect("should succeed");

        let seg = &ctx.segment_audios[0];
        assert_eq!(seg.extract_end_sample, 1600);
    }

    #[test]
    fn rejects_empty_plans() {
        let mut ctx = ctx_with_plans(vec![0.0; 100], vec![]);
        let stage = SegmentExtractionStage;
        assert!(stage.execute(&mut ctx).is_err());
    }

    #[test]
    fn resample_linear_identity() {
        let src = vec![1.0, 2.0, 3.0, 4.0];
        let out = resample_linear(&src, 4);
        for (a, b) in src.iter().zip(out.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn resample_linear_stretch() {
        let src = vec![0.0, 1.0];
        let out = resample_linear(&src, 5);
        assert_eq!(out.len(), 5);
        assert!((out[0] - 0.0).abs() < 1e-6);
        assert!((out[4] - 1.0).abs() < 1e-6);
        assert!((out[2] - 0.5).abs() < 1e-6);
    }
}
