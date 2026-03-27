use tempo_domain::{DomainError, TempoPipelineContext, TempoPipelineStage};

const CROSSFADE_BOUNDARY_SAMPLES: usize = 32;

/// Step 14: reassemble modified segments into the global output signal.
///
/// Builds the output buffer sequentially: copies unmodified audio before each
/// segment, inserts the segment's resynthesized local_samples (trimmed to
/// exclude extraction margins), and appends unmodified audio after the last
/// segment. Applies short crossfades at segment boundaries. Writes the final
/// result back into `context.samples`.
pub struct RecombinationStage;

impl TempoPipelineStage for RecombinationStage {
    fn name(&self) -> &'static str {
        "recombination"
    }

    fn execute(&self, context: &mut TempoPipelineContext) -> Result<(), DomainError> {
        if context.segment_audios.is_empty() || context.segment_plans.is_empty() {
            return Err(DomainError::internal_error(
                "recombination: no segment audios or plans available",
            ));
        }

        let original = context.samples.clone();
        let mut output: Vec<f32> = Vec::new();
        let mut read_cursor = 0usize;

        for (seg_idx, seg_audio) in context.segment_audios.iter().enumerate() {
            let plan = match context.segment_plans.get(seg_idx) {
                Some(p) => p,
                None => continue,
            };

            // Copy unmodified audio before this segment
            let seg_start = plan.start_sample.min(original.len());
            if read_cursor < seg_start {
                output.extend_from_slice(&original[read_cursor..seg_start]);
            }

            // Extract the useful portion of local_samples (trim margins)
            let margin_left = seg_audio.margin_left;
            let margin_right = seg_audio.margin_right;
            let local = &seg_audio.local_samples;
            let useful_start = margin_left.min(local.len());
            let useful_end = local.len().saturating_sub(margin_right);

            if useful_end > useful_start {
                let segment_samples = &local[useful_start..useful_end];

                // Apply boundary crossfade at the start
                let boundary_start = output.len();
                output.extend_from_slice(segment_samples);

                if boundary_start > 0 && boundary_start < output.len() {
                    apply_boundary_fade(&mut output, boundary_start, CROSSFADE_BOUNDARY_SAMPLES);
                }
            }

            read_cursor = plan.end_sample.min(original.len());
        }

        // Copy remaining unmodified audio after the last segment
        if read_cursor < original.len() {
            let boundary = output.len();
            output.extend_from_slice(&original[read_cursor..]);
            if boundary > 0 && boundary < output.len() {
                apply_boundary_fade(&mut output, boundary, CROSSFADE_BOUNDARY_SAMPLES);
            }
        }

        let original_len = original.len();
        let output_len = output.len();
        tracing::debug!(
            original_len,
            output_len,
            sample_rate_hz = context.sample_rate_hz,
            segment_count = context.segment_audios.len(),
            "global recombination complete"
        );

        context.samples = output;
        Ok(())
    }
}

/// Apply a short equal-power crossfade around a splice boundary.
fn apply_boundary_fade(output: &mut [f32], boundary: usize, half_len: usize) {
    let n = output.len();
    if boundary == 0 || boundary >= n || half_len == 0 {
        return;
    }

    let fade_start = boundary.saturating_sub(half_len);
    let fade_end = (boundary + half_len).min(n);
    let total = fade_end - fade_start;

    if total < 2 {
        return;
    }

    for i in 0..total {
        let t = i as f32 / (total - 1) as f32;
        let weight = 0.5 * (1.0 - (std::f32::consts::PI * t).cos());
        let idx = fade_start + i;
        if idx < boundary {
            output[idx] *= weight;
        } else {
            output[idx] *= 1.0 - weight + weight;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempo_domain::{SegmentAudio, SegmentPlan, TempoPipelineContext};

    fn make_ctx(
        original: Vec<f32>,
        segments: Vec<(SegmentPlan, SegmentAudio)>,
    ) -> TempoPipelineContext {
        let mut ctx =
            TempoPipelineContext::new(original, 16_000, Vec::new(), Vec::new());
        let (plans, audios): (Vec<_>, Vec<_>) = segments.into_iter().unzip();
        ctx.segment_plans = plans;
        ctx.segment_audios = audios;
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

    fn audio(samples: Vec<f32>, global_start: usize, global_end: usize) -> SegmentAudio {
        SegmentAudio {
            local_samples: samples,
            global_start_sample: global_start,
            global_end_sample: global_end,
            margin_left: 0,
            margin_right: 0,
            target_duration_samples: global_end - global_start,
            alpha: 1.0,
        }
    }

    #[test]
    fn single_segment_replaces_range() {
        let original = vec![0.1; 1000];
        let modified = vec![0.9; 500];
        let mut ctx = make_ctx(
            original,
            vec![(plan(200, 700), audio(modified, 200, 700))],
        );

        let stage = RecombinationStage;
        stage.execute(&mut ctx).expect("should succeed");

        // Before segment: original values
        assert!((ctx.samples[0] - 0.1).abs() < 0.2);
        // After segment: original values
        assert!((ctx.samples[ctx.samples.len() - 1] - 0.1).abs() < 0.2);
        // Total length: 200 (before) + 500 (segment) + 300 (after) = 1000
        assert_eq!(ctx.samples.len(), 1000);
    }

    #[test]
    fn stretched_segment_changes_output_length() {
        let original = vec![0.1; 1000];
        let modified = vec![0.9; 750]; // 50% longer than 500
        let mut ctx = make_ctx(
            original,
            vec![(plan(200, 700), audio(modified, 200, 700))],
        );

        let stage = RecombinationStage;
        stage.execute(&mut ctx).expect("should succeed");

        // 200 (before) + 750 (stretched segment) + 300 (after) = 1250
        assert_eq!(ctx.samples.len(), 1250);
    }

    #[test]
    fn multiple_segments_assembled_in_order() {
        let original = vec![0.0; 1000];
        let seg1 = vec![1.0; 200];
        let seg2 = vec![2.0; 200];
        let mut ctx = make_ctx(
            original,
            vec![
                (plan(100, 300), audio(seg1, 100, 300)),
                (plan(500, 700), audio(seg2, 500, 700)),
            ],
        );

        let stage = RecombinationStage;
        stage.execute(&mut ctx).expect("should succeed");

        // 100 + 200 + 200 (gap) + 200 + 300 = 1000
        assert_eq!(ctx.samples.len(), 1000);
        // First segment region should have 1.0 values (approximately, crossfade at edges)
        assert!((ctx.samples[150] - 1.0).abs() < 0.3);
        // Second segment region
        assert!((ctx.samples[550] - 2.0).abs() < 0.3);
    }

    #[test]
    fn margins_are_trimmed() {
        let original = vec![0.0; 400];
        let local = vec![0.5; 220]; // 10 margin_left + 200 useful + 10 margin_right
        let mut seg = audio(local, 100, 300);
        seg.margin_left = 10;
        seg.margin_right = 10;
        let mut ctx = make_ctx(original, vec![(plan(100, 300), seg)]);

        let stage = RecombinationStage;
        stage.execute(&mut ctx).expect("should succeed");

        // 100 (before) + 200 (trimmed segment) + 100 (after) = 400
        assert_eq!(ctx.samples.len(), 400);
    }

    #[test]
    fn rejects_empty_segment_audios() {
        let mut ctx = TempoPipelineContext::new(vec![0.0; 100], 16_000, Vec::new(), Vec::new());
        let stage = RecombinationStage;
        assert!(stage.execute(&mut ctx).is_err());
    }
}
