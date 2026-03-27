use tempo_domain::{DomainError, TempoPipelineContext, TempoPipelineStage};

/// Step 15: log structured diagnostic data for each segment via tracing.
///
/// Reports per-segment: source/target/obtained duration, F0 statistics,
/// pitch mark count, voiced frame ratio, and stretch region breakdown.
/// All output goes through `tracing` -- no file I/O.
pub struct DebugExportStage;

impl TempoPipelineStage for DebugExportStage {
    fn name(&self) -> &'static str {
        "debug_export"
    }

    fn execute(&self, context: &mut TempoPipelineContext) -> Result<(), DomainError> {
        let rate = context.sample_rate_hz;

        for (seg_idx, plan) in context.segment_plans.iter().enumerate() {
            let source_duration_ms = samples_to_ms(plan.original_duration_samples, rate);
            let target_duration_ms = samples_to_ms(plan.target_duration_samples, rate);

            let obtained_samples = context
                .segment_audios
                .get(seg_idx)
                .map(|a| {
                    let useful = a.local_samples.len()
                        .saturating_sub(a.margin_left)
                        .saturating_sub(a.margin_right);
                    useful
                })
                .unwrap_or(0);
            let obtained_duration_ms = samples_to_ms(obtained_samples, rate);

            let (f0_mean, f0_min, f0_max) = f0_stats(context, seg_idx);
            let pitch_mark_count = context
                .pitch_marks
                .get(seg_idx)
                .map(|pm| pm.marks.len())
                .unwrap_or(0);

            let (total_frames, voiced_frames) = voiced_ratio(context, seg_idx);
            let voiced_pct = if total_frames > 0 {
                (voiced_frames as f32 / total_frames as f32) * 100.0
            } else {
                0.0
            };

            let stretch_summary = stretch_breakdown(context, seg_idx);

            tracing::info!(
                segment_index = seg_idx,
                source_duration_ms,
                target_duration_ms,
                obtained_duration_ms,
                alpha = plan.alpha,
                f0_mean,
                f0_min,
                f0_max,
                pitch_mark_count,
                total_frames,
                voiced_frames,
                voiced_pct,
                stretch_summary = %stretch_summary,
                "segment diagnostic report"
            );
        }

        let output_duration_ms = samples_to_ms(context.samples.len(), rate);
        let input_duration_ms = samples_to_ms(
            context.segment_plans.iter().map(|p| p.original_duration_samples).sum::<usize>()
                + untreated_samples(context),
            rate,
        );

        tracing::info!(
            segment_count = context.segment_plans.len(),
            output_samples = context.samples.len(),
            output_duration_ms,
            input_duration_ms,
            sample_rate_hz = rate,
            "pipeline debug export complete"
        );

        Ok(())
    }
}

fn samples_to_ms(samples: usize, rate: u32) -> u64 {
    if rate == 0 {
        return 0;
    }
    (samples as u64 * 1000) / rate as u64
}

fn f0_stats(context: &TempoPipelineContext, seg_idx: usize) -> (f32, f32, f32) {
    let frames = match context.pitch_data.get(seg_idx) {
        Some(pd) => &pd.frames,
        None => return (0.0, 0.0, 0.0),
    };

    let voiced_f0: Vec<f32> = frames.iter().filter(|f| f.voiced).map(|f| f.f0_hz).collect();
    if voiced_f0.is_empty() {
        return (0.0, 0.0, 0.0);
    }

    let mean = voiced_f0.iter().sum::<f32>() / voiced_f0.len() as f32;
    let min = voiced_f0.iter().cloned().fold(f32::MAX, f32::min);
    let max = voiced_f0.iter().cloned().fold(f32::MIN, f32::max);
    (mean, min, max)
}

fn voiced_ratio(context: &TempoPipelineContext, seg_idx: usize) -> (usize, usize) {
    match context.frame_analyses.get(seg_idx) {
        Some(fa) => {
            let total = fa.frames.len();
            let voiced = fa.frames.iter().filter(|f| f.is_voiced).count();
            (total, voiced)
        }
        None => (0, 0),
    }
}

fn stretch_breakdown(context: &TempoPipelineContext, seg_idx: usize) -> String {
    let plan = match context.stretch_plans.get(seg_idx) {
        Some(p) => p,
        None => return "no stretch plan".to_string(),
    };

    let mut pause_count = 0usize;
    let mut voiced_count = 0usize;
    let mut keep_count = 0usize;

    for r in &plan.regions {
        match r.mode {
            tempo_domain::StretchMode::Pause => pause_count += 1,
            tempo_domain::StretchMode::VoicedPsola => voiced_count += 1,
            tempo_domain::StretchMode::KeepNearConstant => keep_count += 1,
        }
    }

    format!(
        "voiced_psola={} pause={} keep_near_constant={}",
        voiced_count, pause_count, keep_count
    )
}

fn untreated_samples(context: &TempoPipelineContext) -> usize {
    if context.segment_plans.is_empty() {
        return context.samples.len();
    }

    let mut total = 0usize;
    let mut cursor = 0usize;
    for plan in &context.segment_plans {
        if plan.start_sample > cursor {
            total += plan.start_sample - cursor;
        }
        cursor = plan.end_sample;
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempo_domain::{
        FrameMetrics, PitchFrame, PitchMark, SegmentAudio, SegmentFrameAnalysis,
        SegmentPitchData, SegmentPitchMarks, SegmentPlan, SegmentStretchPlan, StretchMode,
        StretchRegion, TempoPipelineContext,
    };

    fn make_ctx() -> TempoPipelineContext {
        let mut ctx = TempoPipelineContext::new(vec![0.5; 1600], 16_000, Vec::new(), Vec::new());
        ctx.segment_plans = vec![SegmentPlan {
            start_sample: 0,
            end_sample: 1600,
            original_duration_samples: 1600,
            target_duration_samples: 2000,
            alpha: 1.25,
        }];
        ctx.segment_audios = vec![SegmentAudio {
            local_samples: vec![0.5; 2000],
            global_start_sample: 0,
            global_end_sample: 1600,
            margin_left: 0,
            margin_right: 0,
            target_duration_samples: 2000,
            alpha: 1.25,
        }];
        ctx.frame_analyses = vec![SegmentFrameAnalysis {
            segment_index: 0,
            frame_length_samples: 480,
            hop_samples: 160,
            frames: vec![
                FrameMetrics { energy: 0.5, is_voiced: true, periodicity: 0.9 },
                FrameMetrics { energy: 0.5, is_voiced: true, periodicity: 0.9 },
                FrameMetrics { energy: 0.01, is_voiced: false, periodicity: 0.1 },
            ],
        }];
        ctx.pitch_data = vec![SegmentPitchData {
            segment_index: 0,
            frames: vec![
                PitchFrame { center_sample: 240, voiced: true, f0_hz: 200.0, period_samples: 80.0 },
                PitchFrame { center_sample: 400, voiced: true, f0_hz: 210.0, period_samples: 76.0 },
                PitchFrame { center_sample: 560, voiced: false, f0_hz: 0.0, period_samples: 0.0 },
            ],
        }];
        ctx.pitch_marks = vec![SegmentPitchMarks {
            segment_index: 0,
            marks: vec![
                PitchMark { sample_index: 200, local_period_samples: 80.0, confidence: 0.9 },
                PitchMark { sample_index: 280, local_period_samples: 80.0, confidence: 0.85 },
            ],
        }];
        ctx.stretch_plans = vec![SegmentStretchPlan {
            segment_index: 0,
            regions: vec![
                StretchRegion { start_sample: 0, end_sample: 800, local_alpha: 1.3, mode: StretchMode::VoicedPsola },
                StretchRegion { start_sample: 800, end_sample: 1600, local_alpha: 1.1, mode: StretchMode::KeepNearConstant },
            ],
        }];
        ctx
    }

    #[test]
    fn debug_export_succeeds_with_full_context() {
        let mut ctx = make_ctx();
        let stage = DebugExportStage;
        stage.execute(&mut ctx).expect("should succeed");
    }

    #[test]
    fn debug_export_succeeds_with_minimal_context() {
        let mut ctx = TempoPipelineContext::new(vec![0.0; 100], 16_000, Vec::new(), Vec::new());
        ctx.segment_plans = vec![SegmentPlan {
            start_sample: 0,
            end_sample: 100,
            original_duration_samples: 100,
            target_duration_samples: 100,
            alpha: 1.0,
        }];
        ctx.segment_audios = vec![SegmentAudio {
            local_samples: vec![0.0; 100],
            global_start_sample: 0,
            global_end_sample: 100,
            margin_left: 0,
            margin_right: 0,
            target_duration_samples: 100,
            alpha: 1.0,
        }];

        let stage = DebugExportStage;
        stage.execute(&mut ctx).expect("should succeed with minimal data");
    }

    #[test]
    fn f0_stats_computes_correct_values() {
        let ctx = make_ctx();
        let (mean, min, max) = f0_stats(&ctx, 0);
        assert!((mean - 205.0).abs() < 1.0);
        assert!((min - 200.0).abs() < 1.0);
        assert!((max - 210.0).abs() < 1.0);
    }

    #[test]
    fn voiced_ratio_correct() {
        let ctx = make_ctx();
        let (total, voiced) = voiced_ratio(&ctx, 0);
        assert_eq!(total, 3);
        assert_eq!(voiced, 2);
    }

    #[test]
    fn stretch_breakdown_format() {
        let ctx = make_ctx();
        let s = stretch_breakdown(&ctx, 0);
        assert!(s.contains("voiced_psola=1"));
        assert!(s.contains("keep_near_constant=1"));
        assert!(s.contains("pause=0"));
    }
}
