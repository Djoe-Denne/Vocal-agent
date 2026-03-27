use serde::Serialize;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::PathBuf;
use tempo_domain::{DomainError, SegmentKind, StretchMode, TempoPipelineContext, TempoPipelineStage};
use uuid::Uuid;

/// Step 15: write structured debug artifacts to `./debug-dump/{session}/`.
///
/// Produces numbered JSON files for every pipeline data structure plus a
/// human/LLM-readable `narrative.md` summarising what happened and flagging
/// anomalies.  Also retains the original `tracing::info!` diagnostics.
pub struct DebugExportStage;

const DUMP_ROOT: &str = "./debug-dump";

impl TempoPipelineStage for DebugExportStage {
    fn name(&self) -> &'static str {
        "debug_export"
    }

    fn execute(&self, context: &mut TempoPipelineContext) -> Result<(), DomainError> {
        let session_id = Uuid::new_v4().to_string();
        let session_dir = PathBuf::from(DUMP_ROOT).join(&session_id);

        if let Err(e) = fs::create_dir_all(&session_dir) {
            tracing::warn!(error = %e, "debug_export: could not create dump directory, skipping file export");
            log_tracing_summary(context);
            return Ok(());
        }

        tracing::info!(session_id = %session_id, path = %session_dir.display(), "debug_export: writing debug dump");

        write_json(&session_dir, "00_input_timings.json", &InputTimings {
            original_timings: &context.original_timings,
            tts_timings: &context.tts_timings,
        });

        write_json(&session_dir, "01_segment_plans.json", &context.segment_plans);
        write_json(&session_dir, "02_segment_audios_meta.json", &segment_audio_metas(context));
        write_json(&session_dir, "03_frame_analyses.json", &context.frame_analyses);
        write_json(&session_dir, "04_pitch_data.json", &context.pitch_data);
        write_json(&session_dir, "05_voiced_regions.json", &context.voiced_regions);
        write_json(&session_dir, "06_pitch_marks.json", &context.pitch_marks);
        write_json(&session_dir, "07_stretch_plans.json", &context.stretch_plans);
        write_json(&session_dir, "08_grains_meta.json", &grains_meta(context));
        write_json(&session_dir, "09_synthesis_grids.json", &context.synthesis_grids);
        write_json(&session_dir, "10_synthesis_plans.json", &context.synthesis_plans);

        let summary = build_pipeline_summary(context);
        write_json(&session_dir, "11_pipeline_summary.json", &summary);

        let narrative = build_narrative(context, &summary);
        let narrative_path = session_dir.join("narrative.md");
        if let Err(e) = fs::write(&narrative_path, &narrative) {
            tracing::warn!(error = %e, "debug_export: failed to write narrative.md");
        }

        log_tracing_summary(context);

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// JSON helpers
// ---------------------------------------------------------------------------

fn write_json<T: Serialize>(dir: &PathBuf, filename: &str, value: &T) {
    let path = dir.join(filename);
    match serde_json::to_string_pretty(value) {
        Ok(json) => {
            if let Err(e) = fs::write(&path, json) {
                tracing::warn!(file = filename, error = %e, "debug_export: failed to write file");
            }
        }
        Err(e) => {
            tracing::warn!(file = filename, error = %e, "debug_export: serialization failed");
        }
    }
}

// ---------------------------------------------------------------------------
// Lightweight metadata structs
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct InputTimings<'a> {
    original_timings: &'a [tempo_domain::WordTiming],
    tts_timings: &'a [tempo_domain::WordTiming],
}

#[derive(Serialize)]
struct SegmentAudioMeta {
    segment_index: usize,
    kind: String,
    analysis_samples_len: usize,
    rendered_samples_len: usize,
    global_start_sample: usize,
    global_end_sample: usize,
    extract_start_sample: usize,
    extract_end_sample: usize,
    useful_start_in_analysis: usize,
    useful_end_in_analysis: usize,
    useful_len: usize,
    target_duration_samples: usize,
    alpha: f64,
    rms_energy: f32,
    peak_amplitude: f32,
}

fn segment_audio_metas(ctx: &TempoPipelineContext) -> Vec<SegmentAudioMeta> {
    ctx.segment_audios
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let useful_len = a.useful_end_in_analysis.saturating_sub(a.useful_start_in_analysis);
            let buf = if !a.rendered_samples.is_empty() { &a.rendered_samples } else { &a.analysis_samples };
            let rms = if buf.is_empty() {
                0.0
            } else {
                (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt()
            };
            let peak = buf.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
            SegmentAudioMeta {
                segment_index: i,
                kind: format!("{:?}", a.kind),
                analysis_samples_len: a.analysis_samples.len(),
                rendered_samples_len: a.rendered_samples.len(),
                global_start_sample: a.global_start_sample,
                global_end_sample: a.global_end_sample,
                extract_start_sample: a.extract_start_sample,
                extract_end_sample: a.extract_end_sample,
                useful_start_in_analysis: a.useful_start_in_analysis,
                useful_end_in_analysis: a.useful_end_in_analysis,
                useful_len,
                target_duration_samples: a.target_duration_samples,
                alpha: a.alpha,
                rms_energy: rms,
                peak_amplitude: peak,
            }
        })
        .collect()
}

#[derive(Serialize)]
struct GrainMeta {
    segment_index: usize,
    grain_index: usize,
    analysis_mark_index: usize,
    center_sample: usize,
    window_length: usize,
}

fn grains_meta(ctx: &TempoPipelineContext) -> Vec<GrainMeta> {
    ctx.grains
        .iter()
        .flat_map(|sg| {
            sg.grains.iter().enumerate().map(move |(gi, g)| GrainMeta {
                segment_index: sg.segment_index,
                grain_index: gi,
                analysis_mark_index: g.analysis_mark_index,
                center_sample: g.center_sample,
                window_length: g.windowed_samples.len(),
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Pipeline summary
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct PipelineSummary {
    sample_rate_hz: u32,
    input_samples: usize,
    input_duration_ms: u64,
    output_samples: usize,
    output_duration_ms: u64,
    duration_delta_ms: i64,
    segment_count: usize,
    word_segment_count: usize,
    gap_segment_count: usize,
    segments: Vec<SegmentSummary>,
    anomalies: Vec<String>,
}

#[derive(Serialize)]
struct SegmentSummary {
    index: usize,
    kind: String,
    label: String,
    tts_duration_ms: u64,
    original_duration_ms: u64,
    alpha: f64,
    source_duration_ms: u64,
    target_duration_ms: u64,
    obtained_duration_ms: u64,
    duration_error_ms: i64,
    f0_mean: f32,
    f0_min: f32,
    f0_max: f32,
    pitch_mark_count: usize,
    total_frames: usize,
    voiced_frames: usize,
    voiced_pct: f32,
    voiced_region_count: usize,
    stretch_regions: StretchBreakdown,
    grain_count: usize,
    synthesis_placement_count: usize,
}

#[derive(Serialize)]
struct StretchBreakdown {
    pause: usize,
    voiced_psola: usize,
    keep_near_constant: usize,
}

fn build_pipeline_summary(ctx: &TempoPipelineContext) -> PipelineSummary {
    let rate = ctx.sample_rate_hz;
    let input_samples = ctx.segment_plans.iter().map(|p| p.original_duration_samples).sum::<usize>()
        + untreated_samples(ctx);
    let output_samples = ctx.samples.len();

    let mut segments = Vec::new();
    let mut anomalies = Vec::new();

    for (seg_idx, plan) in ctx.segment_plans.iter().enumerate() {
        let label = plan.label.clone().unwrap_or_default();
        let tts_duration_ms = plan.tts_end_ms.saturating_sub(plan.tts_start_ms);
        let original_duration_ms = plan.original_end_ms.saturating_sub(plan.original_start_ms);

        let source_duration_ms = samples_to_ms(plan.original_duration_samples, rate);
        let target_duration_ms = samples_to_ms(plan.target_duration_samples, rate);

        let obtained_samples = ctx.segment_audios.get(seg_idx)
            .map(|a| a.rendered_samples.len())
            .unwrap_or(0);
        let obtained_duration_ms = samples_to_ms(obtained_samples, rate);
        let duration_error_ms = obtained_duration_ms as i64 - target_duration_ms as i64;

        let (f0_mean, f0_min, f0_max) = f0_stats(ctx, seg_idx);
        let pitch_mark_count = ctx.pitch_marks.get(seg_idx).map(|pm| pm.marks.len()).unwrap_or(0);
        let (total_frames, voiced_frames) = voiced_ratio(ctx, seg_idx);
        let voiced_pct = if total_frames > 0 { (voiced_frames as f32 / total_frames as f32) * 100.0 } else { 0.0 };

        let voiced_region_count = ctx.voiced_regions.get(seg_idx).map(|vr| vr.regions.len()).unwrap_or(0);
        let stretch = stretch_breakdown(ctx, seg_idx);
        let grain_count = ctx.grains.get(seg_idx).map(|g| g.grains.len()).unwrap_or(0);
        let synthesis_placement_count = ctx.synthesis_plans.get(seg_idx).map(|sp| sp.placements.len()).unwrap_or(0);

        if plan.kind == SegmentKind::Word {
            if plan.alpha > 2.0 {
                anomalies.push(format!("Segment {} (\"{}\"): alpha={:.2} is very high (>2x stretch)", seg_idx, label, plan.alpha));
            }
            if plan.alpha < 0.5 {
                anomalies.push(format!("Segment {} (\"{}\"): alpha={:.2} is very low (<0.5x compression)", seg_idx, label, plan.alpha));
            }
            if voiced_pct < 10.0 && total_frames > 0 {
                anomalies.push(format!("Segment {} (\"{}\"): only {:.0}% voiced frames", seg_idx, label, voiced_pct));
            }
            if pitch_mark_count == 0 && voiced_region_count > 0 {
                anomalies.push(format!("Segment {} (\"{}\"): voiced regions but no pitch marks", seg_idx, label));
            }
            if duration_error_ms.unsigned_abs() > 50 {
                anomalies.push(format!("Segment {} (\"{}\"): duration error {}ms", seg_idx, label, duration_error_ms));
            }
        }

        segments.push(SegmentSummary {
            index: seg_idx,
            kind: format!("{:?}", plan.kind),
            label,
            tts_duration_ms,
            original_duration_ms,
            alpha: plan.alpha,
            source_duration_ms,
            target_duration_ms,
            obtained_duration_ms,
            duration_error_ms,
            f0_mean,
            f0_min,
            f0_max,
            pitch_mark_count,
            total_frames,
            voiced_frames,
            voiced_pct,
            voiced_region_count,
            stretch_regions: stretch,
            grain_count,
            synthesis_placement_count,
        });
    }

    let word_count = ctx.segment_plans.iter().filter(|p| p.kind == SegmentKind::Word).count();
    let gap_count = ctx.segment_plans.iter().filter(|p| p.kind == SegmentKind::Gap).count();

    PipelineSummary {
        sample_rate_hz: rate,
        input_samples,
        input_duration_ms: samples_to_ms(input_samples, rate),
        output_samples,
        output_duration_ms: samples_to_ms(output_samples, rate),
        duration_delta_ms: samples_to_ms(output_samples, rate) as i64 - samples_to_ms(input_samples, rate) as i64,
        segment_count: ctx.segment_plans.len(),
        word_segment_count: word_count,
        gap_segment_count: gap_count,
        segments,
        anomalies,
    }
}

// ---------------------------------------------------------------------------
// Narrative markdown
// ---------------------------------------------------------------------------

fn build_narrative(ctx: &TempoPipelineContext, summary: &PipelineSummary) -> String {
    let mut md = String::with_capacity(4096);

    let _ = writeln!(md, "# Tempo Pipeline Debug Report\n");
    let _ = writeln!(md, "## Input\n");
    let _ = writeln!(md, "- **Sample rate:** {} Hz", summary.sample_rate_hz);
    let _ = writeln!(md, "- **Input duration:** {}ms ({} samples)", summary.input_duration_ms, summary.input_samples);
    let _ = writeln!(md, "- **TTS words:** {}", ctx.tts_timings.len());
    let _ = writeln!(md, "- **Original words:** {}", ctx.original_timings.len());
    let _ = writeln!(md, "- **Segments:** {} total ({} Word, {} Gap)", summary.segment_count, summary.word_segment_count, summary.gap_segment_count);
    let _ = writeln!(md);

    if !ctx.tts_timings.is_empty() {
        let _ = writeln!(md, "### Word Alignment\n");
        let _ = writeln!(md, "| # | TTS Word | TTS (ms) | Original Word | Original (ms) | Delta (ms) |");
        let _ = writeln!(md, "|---|----------|----------|---------------|---------------|------------|");
        let count = ctx.tts_timings.len().min(ctx.original_timings.len());
        for i in 0..count {
            let tw = &ctx.tts_timings[i];
            let ow = &ctx.original_timings[i];
            let tts_dur = tw.end_ms.saturating_sub(tw.start_ms);
            let orig_dur = ow.end_ms.saturating_sub(ow.start_ms);
            let delta = orig_dur as i64 - tts_dur as i64;
            let _ = writeln!(md, "| {} | {} | {} | {} | {} | {:+} |", i, tw.word, tts_dur, ow.word, orig_dur, delta);
        }
        let _ = writeln!(md);
    }

    let _ = writeln!(md, "## Segment Timeline\n");
    let _ = writeln!(md, "| # | Kind | Label | TTS (ms) | Original (ms) | Alpha | Target (ms) | Obtained (ms) | Error |");
    let _ = writeln!(md, "|---|------|-------|----------|---------------|-------|-------------|---------------|-------|");
    for seg in &summary.segments {
        let _ = writeln!(md, "| {} | {} | {} | {} | {} | {:.2} | {} | {} | {:+} |",
            seg.index, seg.kind, seg.label, seg.tts_duration_ms, seg.original_duration_ms,
            seg.alpha, seg.target_duration_ms, seg.obtained_duration_ms, seg.duration_error_ms);
    }
    let _ = writeln!(md);

    let _ = writeln!(md, "## Word Segment Details\n");
    for seg in summary.segments.iter().filter(|s| s.kind == "Word") {
        let _ = writeln!(md, "### Segment {}: \"{}\"", seg.index, seg.label);
        let _ = writeln!(md);

        let direction = if seg.alpha > 1.01 {
            format!("stretching by {:.0}%", (seg.alpha - 1.0) * 100.0)
        } else if seg.alpha < 0.99 {
            format!("compressing by {:.0}%", (1.0 - seg.alpha) * 100.0)
        } else {
            "near identity".to_string()
        };
        let _ = writeln!(md, "- **Alpha:** {:.3} ({})", seg.alpha, direction);
        let _ = writeln!(md, "- **Source:** {}ms | **Target:** {}ms | **Obtained:** {}ms (error: {:+}ms)",
            seg.source_duration_ms, seg.target_duration_ms, seg.obtained_duration_ms, seg.duration_error_ms);

        if seg.f0_mean > 0.0 {
            let _ = writeln!(md, "- **F0:** mean={:.1}Hz range=[{:.1}, {:.1}]", seg.f0_mean, seg.f0_min, seg.f0_max);
        }
        let _ = writeln!(md, "- **Frames:** {} ({} voiced, {:.0}%)", seg.total_frames, seg.voiced_frames, seg.voiced_pct);
        let _ = writeln!(md, "- **Marks:** {} pitch, {} grains, {} placements", seg.pitch_mark_count, seg.grain_count, seg.synthesis_placement_count);
        let _ = writeln!(md, "- **Stretch:** {} VoicedPsola, {} Pause, {} Keep",
            seg.stretch_regions.voiced_psola, seg.stretch_regions.pause, seg.stretch_regions.keep_near_constant);
        let _ = writeln!(md);
    }

    let _ = writeln!(md, "## Output\n");
    let _ = writeln!(md, "- **Output duration:** {}ms ({} samples)", summary.output_duration_ms, summary.output_samples);
    let _ = writeln!(md, "- **Duration change:** {:+}ms", summary.duration_delta_ms);
    let _ = writeln!(md);

    if !summary.anomalies.is_empty() {
        let _ = writeln!(md, "## Anomalies / Warnings\n");
        for a in &summary.anomalies {
            let _ = writeln!(md, "- {}", a);
        }
        let _ = writeln!(md);
    }

    md
}

// ---------------------------------------------------------------------------
// Tracing summary
// ---------------------------------------------------------------------------

fn log_tracing_summary(context: &TempoPipelineContext) {
    let rate = context.sample_rate_hz;

    for (seg_idx, plan) in context.segment_plans.iter().enumerate() {
        let source_duration_ms = samples_to_ms(plan.original_duration_samples, rate);
        let target_duration_ms = samples_to_ms(plan.target_duration_samples, rate);

        let obtained_samples = context.segment_audios.get(seg_idx)
            .map(|a| a.rendered_samples.len())
            .unwrap_or(0);
        let obtained_duration_ms = samples_to_ms(obtained_samples, rate);

        let (f0_mean, f0_min, f0_max) = f0_stats(context, seg_idx);
        let pitch_mark_count = context.pitch_marks.get(seg_idx).map(|pm| pm.marks.len()).unwrap_or(0);
        let (total_frames, voiced_frames) = voiced_ratio(context, seg_idx);
        let voiced_pct = if total_frames > 0 { (voiced_frames as f32 / total_frames as f32) * 100.0 } else { 0.0 };
        let stretch_summary = stretch_breakdown_str(context, seg_idx);

        tracing::info!(
            segment_index = seg_idx,
            kind = ?plan.kind,
            source_duration_ms,
            target_duration_ms,
            obtained_duration_ms,
            alpha = plan.alpha,
            f0_mean, f0_min, f0_max,
            pitch_mark_count,
            total_frames, voiced_frames, voiced_pct,
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
        output_duration_ms, input_duration_ms,
        sample_rate_hz = rate,
        "pipeline debug export complete"
    );
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn samples_to_ms(samples: usize, rate: u32) -> u64 {
    if rate == 0 { return 0; }
    (samples as u64 * 1000) / rate as u64
}

fn f0_stats(context: &TempoPipelineContext, seg_idx: usize) -> (f32, f32, f32) {
    let frames = match context.pitch_data.get(seg_idx) {
        Some(pd) => &pd.frames,
        None => return (0.0, 0.0, 0.0),
    };
    let voiced_f0: Vec<f32> = frames.iter().filter(|f| f.voiced).map(|f| f.f0_hz).collect();
    if voiced_f0.is_empty() { return (0.0, 0.0, 0.0); }
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

fn stretch_breakdown(context: &TempoPipelineContext, seg_idx: usize) -> StretchBreakdown {
    let plan = match context.stretch_plans.get(seg_idx) {
        Some(p) => p,
        None => return StretchBreakdown { pause: 0, voiced_psola: 0, keep_near_constant: 0 },
    };
    let mut pause = 0usize;
    let mut voiced_psola = 0usize;
    let mut keep_near_constant = 0usize;
    for r in &plan.regions {
        match r.mode {
            StretchMode::Pause => pause += 1,
            StretchMode::VoicedPsola => voiced_psola += 1,
            StretchMode::KeepNearConstant => keep_near_constant += 1,
        }
    }
    StretchBreakdown { pause, voiced_psola, keep_near_constant }
}

fn stretch_breakdown_str(context: &TempoPipelineContext, seg_idx: usize) -> String {
    let b = stretch_breakdown(context, seg_idx);
    format!("voiced_psola={} pause={} keep_near_constant={}", b.voiced_psola, b.pause, b.keep_near_constant)
}

fn untreated_samples(context: &TempoPipelineContext) -> usize {
    if context.segment_plans.is_empty() { return context.samples.len(); }
    let mut total = 0usize;
    let mut cursor = 0usize;
    for plan in &context.segment_plans {
        if plan.start_sample > cursor { total += plan.start_sample - cursor; }
        cursor = plan.end_sample;
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempo_domain::{
        FrameMetrics, PitchFrame, PitchMark, SegmentAudio, SegmentFrameAnalysis,
        SegmentKind, SegmentPitchData, SegmentPitchMarks, SegmentPlan, SegmentStretchPlan,
        StretchMode, StretchRegion, TempoPipelineContext, WordTiming,
    };

    fn make_ctx() -> TempoPipelineContext {
        let mut ctx = TempoPipelineContext::new(
            vec![0.5; 1600],
            16_000,
            vec![WordTiming { word: "hello".into(), start_ms: 0, end_ms: 600, confidence: 0.95 }],
            vec![WordTiming { word: "hello".into(), start_ms: 0, end_ms: 500, confidence: 0.90 }],
        );
        ctx.segment_plans = vec![SegmentPlan {
            kind: SegmentKind::Word,
            start_sample: 0,
            end_sample: 1600,
            original_duration_samples: 1600,
            target_duration_samples: 2000,
            alpha: 1.25,
            tts_start_ms: 0, tts_end_ms: 500,
            original_start_ms: 0, original_end_ms: 600,
            label: Some("hello".into()),
        }];
        ctx.segment_audios = vec![SegmentAudio {
            analysis_samples: vec![0.5; 2000],
            rendered_samples: vec![0.5; 2000],
            global_start_sample: 0,
            global_end_sample: 1600,
            extract_start_sample: 0,
            extract_end_sample: 1600,
            useful_start_in_analysis: 0,
            useful_end_in_analysis: 2000,
            target_duration_samples: 2000,
            alpha: 1.25,
            kind: SegmentKind::Word,
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
            kind: SegmentKind::Word,
            start_sample: 0, end_sample: 100,
            original_duration_samples: 100, target_duration_samples: 100,
            alpha: 1.0,
            tts_start_ms: 0, tts_end_ms: 0,
            original_start_ms: 0, original_end_ms: 0,
            label: None,
        }];
        ctx.segment_audios = vec![SegmentAudio {
            analysis_samples: vec![0.0; 100],
            rendered_samples: vec![0.0; 100],
            global_start_sample: 0, global_end_sample: 100,
            extract_start_sample: 0, extract_end_sample: 100,
            useful_start_in_analysis: 0, useful_end_in_analysis: 100,
            target_duration_samples: 100, alpha: 1.0,
            kind: SegmentKind::Word,
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
    fn stretch_breakdown_counts() {
        let ctx = make_ctx();
        let b = stretch_breakdown(&ctx, 0);
        assert_eq!(b.voiced_psola, 1);
        assert_eq!(b.keep_near_constant, 1);
        assert_eq!(b.pause, 0);
    }

    #[test]
    fn narrative_contains_key_sections() {
        let ctx = make_ctx();
        let summary = build_pipeline_summary(&ctx);
        let narrative = build_narrative(&ctx, &summary);
        assert!(narrative.contains("# Tempo Pipeline Debug Report"));
        assert!(narrative.contains("## Input"));
        assert!(narrative.contains("## Output"));
        assert!(narrative.contains("hello"));
        assert!(narrative.contains("Word"));
    }

    #[test]
    fn summary_detects_anomalies() {
        let mut ctx = make_ctx();
        ctx.segment_plans[0].alpha = 3.0;
        let summary = build_pipeline_summary(&ctx);
        assert!(!summary.anomalies.is_empty());
        assert!(summary.anomalies.iter().any(|a| a.contains("very high")));
    }
}
