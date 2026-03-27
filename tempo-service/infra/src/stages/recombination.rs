use tempo_domain::{DomainError, TempoPipelineContext, TempoPipelineStage};

const CROSSFADE_BOUNDARY_SAMPLES: usize = 32;

/// Step 14: reassemble modified segments into the global output signal.
///
/// Reads `rendered_samples` from each segment (already margin-free) and
/// splices them into the output buffer with boundary crossfades.
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

            let seg_start = plan.start_sample.min(original.len());
            if read_cursor < seg_start {
                output.extend_from_slice(&original[read_cursor..seg_start]);
            }

            let segment_samples = &seg_audio.rendered_samples;
            if !segment_samples.is_empty() {
                let actual_len = segment_samples.len();

                tracing::debug!(
                    segment_index = seg_idx,
                    kind = ?plan.kind,
                    target_samples = plan.target_duration_samples,
                    actual_samples = actual_len,
                    delta_samples = actual_len as i64 - plan.target_duration_samples as i64,
                    "recombination: segment splice"
                );

                let boundary_start = output.len();
                output.extend_from_slice(segment_samples);

                if boundary_start > 0 && boundary_start < output.len() {
                    apply_boundary_fade(&mut output, boundary_start, CROSSFADE_BOUNDARY_SAMPLES);
                }
            }

            read_cursor = plan.end_sample.min(original.len());
        }

        if read_cursor < original.len() {
            let boundary = output.len();
            output.extend_from_slice(&original[read_cursor..]);
            if boundary > 0 && boundary < output.len() {
                apply_boundary_fade(&mut output, boundary, CROSSFADE_BOUNDARY_SAMPLES);
            }
        }

        let original_len = original.len();
        let output_len = output.len();
        let rate = context.sample_rate_hz;
        let original_ms = if rate > 0 { (original_len as u64 * 1000) / rate as u64 } else { 0 };
        let output_ms = if rate > 0 { (output_len as u64 * 1000) / rate as u64 } else { 0 };

        tracing::debug!(
            original_len,
            output_len,
            original_duration_ms = original_ms,
            output_duration_ms = output_ms,
            delta_ms = output_ms as i64 - original_ms as i64,
            sample_rate_hz = rate,
            segment_count = context.segment_audios.len(),
            "global recombination complete"
        );

        context.samples = output;
        Ok(())
    }
}

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
    use tempo_domain::{SegmentAudio, SegmentKind, SegmentPlan, TempoPipelineContext};

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
            label: None,
        }
    }

    fn audio(rendered: Vec<f32>, global_start: usize, global_end: usize) -> SegmentAudio {
        let n = rendered.len();
        SegmentAudio {
            analysis_samples: Vec::new(),
            rendered_samples: rendered,
            global_start_sample: global_start,
            global_end_sample: global_end,
            extract_start_sample: global_start,
            extract_end_sample: global_end,
            useful_start_in_analysis: 0,
            useful_end_in_analysis: n,
            target_duration_samples: global_end - global_start,
            alpha: 1.0,
            kind: SegmentKind::Word,
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

        assert!((ctx.samples[0] - 0.1).abs() < 0.2);
        assert!((ctx.samples[ctx.samples.len() - 1] - 0.1).abs() < 0.2);
        assert_eq!(ctx.samples.len(), 1000);
    }

    #[test]
    fn stretched_segment_changes_output_length() {
        let original = vec![0.1; 1000];
        let modified = vec![0.9; 750];
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

        assert_eq!(ctx.samples.len(), 1000);
        assert!((ctx.samples[150] - 1.0).abs() < 0.3);
        assert!((ctx.samples[550] - 2.0).abs() < 0.3);
    }

    #[test]
    fn rejects_empty_segment_audios() {
        let mut ctx = TempoPipelineContext::new(vec![0.0; 100], 16_000, Vec::new(), Vec::new());
        let stage = RecombinationStage;
        assert!(stage.execute(&mut ctx).is_err());
    }
}
