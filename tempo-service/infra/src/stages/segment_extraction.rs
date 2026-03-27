use tempo_domain::{DomainError, SegmentAudio, TempoPipelineContext, TempoPipelineStage};

const DEFAULT_MARGIN_MS: u64 = 10;

/// Step 3: extract per-segment audio buffers from the global signal.
///
/// For each `SegmentPlan`, copies the relevant samples into a local buffer
/// with a small analysis margin on each side. Preserves global offsets so
/// reconstruction can place the modified segment back correctly.
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

            let local_samples = context.samples[extract_start..extract_end].to_vec();

            audios.push(SegmentAudio {
                local_samples,
                global_start_sample: plan.start_sample,
                global_end_sample: plan.end_sample,
                margin_left,
                margin_right,
                target_duration_samples: plan.target_duration_samples,
                alpha: plan.alpha,
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempo_domain::{SegmentPlan, TempoPipelineContext};

    fn ctx_with_plans(
        samples: Vec<f32>,
        plans: Vec<SegmentPlan>,
    ) -> TempoPipelineContext {
        let mut ctx = TempoPipelineContext::new(samples, 16_000, Vec::new(), Vec::new());
        ctx.segment_plans = plans;
        ctx
    }

    fn plan(start: usize, end: usize) -> SegmentPlan {
        let dur = end - start;
        SegmentPlan {
            start_sample: start,
            end_sample: end,
            original_duration_samples: dur,
            target_duration_samples: dur,
            alpha: 1.0,
        }
    }

    #[test]
    fn extracts_segment_with_margins() {
        let samples: Vec<f32> = (0..1600).map(|i| i as f32 / 1600.0).collect();
        let mut ctx = ctx_with_plans(samples, vec![plan(200, 1400)]);
        let stage = SegmentExtractionStage;
        stage.execute(&mut ctx).expect("should succeed");

        assert_eq!(ctx.segment_audios.len(), 1);
        let seg = &ctx.segment_audios[0];
        assert_eq!(seg.global_start_sample, 200);
        assert_eq!(seg.global_end_sample, 1400);
        assert!(seg.margin_left > 0);
        assert!(seg.margin_right > 0);
        assert_eq!(
            seg.local_samples.len(),
            1200 + seg.margin_left + seg.margin_right
        );
    }

    #[test]
    fn margin_clamped_at_buffer_start() {
        let samples = vec![0.0; 1600];
        let mut ctx = ctx_with_plans(samples, vec![plan(0, 800)]);
        let stage = SegmentExtractionStage;
        stage.execute(&mut ctx).expect("should succeed");

        let seg = &ctx.segment_audios[0];
        assert_eq!(seg.margin_left, 0);
    }

    #[test]
    fn margin_clamped_at_buffer_end() {
        let samples = vec![0.0; 1600];
        let mut ctx = ctx_with_plans(samples, vec![plan(800, 1600)]);
        let stage = SegmentExtractionStage;
        stage.execute(&mut ctx).expect("should succeed");

        let seg = &ctx.segment_audios[0];
        assert_eq!(seg.margin_right, 0);
    }

    #[test]
    fn rejects_empty_plans() {
        let mut ctx = ctx_with_plans(vec![0.0; 100], vec![]);
        let stage = SegmentExtractionStage;
        assert!(stage.execute(&mut ctx).is_err());
    }
}
