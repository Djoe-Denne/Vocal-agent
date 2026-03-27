use tempo_domain::{
    DomainError, PitchMark, SegmentKind, SegmentPitchMarks, TempoPipelineContext,
    TempoPipelineStage,
};

const SEARCH_WINDOW_RATIO: f32 = 0.25;
const MIN_PERIOD_RATIO: f32 = 0.8;
const MAX_PERIOD_RATIO: f32 = 1.2;

/// Step 7: generate pitch marks within voiced regions.
///
/// Uses polarity-consistent peak snapping and enforces inter-mark spacing
/// within `[0.8, 1.2] * local_period`. Local period from the nearest pitch
/// frame is used as the primary step size.
pub struct PitchMarkStage;

impl TempoPipelineStage for PitchMarkStage {
    fn name(&self) -> &'static str {
        "pitch_mark"
    }

    fn execute(&self, context: &mut TempoPipelineContext) -> Result<(), DomainError> {
        if context.voiced_regions.is_empty() {
            return Err(DomainError::internal_error(
                "pitch_mark: no voiced regions available",
            ));
        }
        if context.segment_audios.len() != context.voiced_regions.len() {
            return Err(DomainError::internal_error(
                "pitch_mark: segment_audios and voiced_regions count mismatch",
            ));
        }

        let mut all_marks = Vec::with_capacity(context.voiced_regions.len());

        for (seg_idx, seg_regions) in context.voiced_regions.iter().enumerate() {
            if context.segment_audios[seg_idx].kind == SegmentKind::Gap {
                all_marks.push(SegmentPitchMarks {
                    segment_index: seg_idx,
                    marks: Vec::new(),
                });
                continue;
            }

            let samples = &context.segment_audios[seg_idx].analysis_samples;
            let mut segment_marks: Vec<PitchMark> = Vec::new();

            for region in &seg_regions.regions {
                let period = region.mean_period_samples;
                if period < 2.0 || region.end_sample <= region.start_sample {
                    continue;
                }

                let start = region.start_sample.min(samples.len());
                let end = region.end_sample.min(samples.len());
                if start >= end {
                    continue;
                }

                let region_slice = &samples[start..end];
                let region_marks =
                    generate_marks_for_region(region_slice, start, period, context.pitch_data
                        .get(seg_idx)
                        .map(|pd| &pd.frames[..]));

                segment_marks.extend(region_marks);
            }

            segment_marks.sort_by_key(|m| m.sample_index);

            // Spacing diagnostics
            let (avg_confidence, avg_period, out_of_range_pct) = mark_diagnostics(&segment_marks);

            tracing::debug!(
                segment_index = seg_idx,
                mark_count = segment_marks.len(),
                voiced_region_count = seg_regions.regions.len(),
                avg_confidence,
                avg_period,
                out_of_range_spacing_pct = out_of_range_pct,
                "pitch marks generated for segment"
            );

            all_marks.push(SegmentPitchMarks {
                segment_index: seg_idx,
                marks: segment_marks,
            });
        }

        tracing::debug!(
            segment_count = all_marks.len(),
            total_marks = all_marks.iter().map(|m| m.marks.len()).sum::<usize>(),
            "pitch mark generation complete"
        );

        context.pitch_marks = all_marks;
        Ok(())
    }
}

fn mark_diagnostics(marks: &[PitchMark]) -> (f32, f32, f32) {
    if marks.is_empty() {
        return (0.0, 0.0, 0.0);
    }

    let avg_confidence = marks.iter().map(|m| m.confidence).sum::<f32>() / marks.len() as f32;
    let avg_period = marks.iter().map(|m| m.local_period_samples).sum::<f32>() / marks.len() as f32;

    if marks.len() < 2 {
        return (avg_confidence, avg_period, 0.0);
    }

    let mut out_of_range = 0usize;
    for w in marks.windows(2) {
        let gap = (w[1].sample_index as f32 - w[0].sample_index as f32).abs();
        let local_t0 = w[0].local_period_samples;
        if local_t0 > 0.0 {
            let ratio = gap / local_t0;
            if ratio < MIN_PERIOD_RATIO || ratio > MAX_PERIOD_RATIO {
                out_of_range += 1;
            }
        }
    }
    let out_of_range_pct = (out_of_range as f32 / (marks.len() - 1) as f32) * 100.0;

    (avg_confidence, avg_period, out_of_range_pct)
}

fn generate_marks_for_region(
    region_samples: &[f32],
    global_offset: usize,
    mean_period: f32,
    pitch_frames: Option<&[tempo_domain::PitchFrame]>,
) -> Vec<PitchMark> {
    let n = region_samples.len();
    if n == 0 || mean_period < 2.0 {
        return Vec::new();
    }

    let search_radius = ((mean_period * SEARCH_WINDOW_RATIO).round() as usize).max(1);

    // Find the best starting point near the center of the region
    let center = n / 2;
    let seed_local = snap_to_peak(region_samples, center, search_radius.max(mean_period.round() as usize / 2));
    let seed_polarity = region_samples.get(seed_local).map(|&s| s >= 0.0).unwrap_or(true);
    let seed_period = local_period_at(seed_local + global_offset, mean_period, pitch_frames);

    let mut marks = Vec::new();
    marks.push(PitchMark {
        sample_index: seed_local + global_offset,
        local_period_samples: seed_period,
        confidence: peak_confidence(region_samples, seed_local, search_radius),
    });

    // Propagate right from seed
    let mut prev_local = seed_local;
    let mut pos = seed_local as f64 + seed_period as f64;
    while (pos.round() as usize) < n {
        let target = pos.round() as usize;
        let lp = local_period_at(target + global_offset, mean_period, pitch_frames);
        let snapped = snap_to_peak_polarized(region_samples, target, search_radius, seed_polarity);

        let gap = snapped as f32 - prev_local as f32;
        let (accepted_pos, accepted) = enforce_spacing(snapped, target, gap, lp);

        if !accepted && (accepted_pos as f32 - prev_local as f32) < lp * MIN_PERIOD_RATIO {
            break;
        }

        if accepted_pos < n {
            let final_lp = local_period_at(accepted_pos + global_offset, mean_period, pitch_frames);
            marks.push(PitchMark {
                sample_index: accepted_pos + global_offset,
                local_period_samples: final_lp,
                confidence: peak_confidence(region_samples, accepted_pos, search_radius),
            });
            prev_local = accepted_pos;
            pos = accepted_pos as f64 + final_lp as f64;
        } else {
            break;
        }
    }

    // Propagate left from seed
    prev_local = seed_local;
    pos = seed_local as f64 - seed_period as f64;
    while pos >= 0.0 {
        let target = pos.round() as usize;
        let lp = local_period_at(target + global_offset, mean_period, pitch_frames);
        let snapped = snap_to_peak_polarized(region_samples, target, search_radius, seed_polarity);

        let gap = prev_local as f32 - snapped as f32;
        let (accepted_pos, accepted) = enforce_spacing(snapped, target, gap, lp);

        if !accepted && (prev_local as f32 - accepted_pos as f32) < lp * MIN_PERIOD_RATIO {
            break;
        }

        let final_lp = local_period_at(accepted_pos + global_offset, mean_period, pitch_frames);
        marks.push(PitchMark {
            sample_index: accepted_pos + global_offset,
            local_period_samples: final_lp,
            confidence: peak_confidence(region_samples, accepted_pos, search_radius),
        });
        prev_local = accepted_pos;
        let next = (accepted_pos as f64 - final_lp as f64).min(pos - 1.0);
        pos = next;
    }

    marks.sort_by_key(|m| m.sample_index);
    marks.dedup_by_key(|m| m.sample_index);
    marks
}

/// Check if a candidate gap is within `[MIN_PERIOD_RATIO, MAX_PERIOD_RATIO] * local_period`.
/// Returns `(final_position, was_accepted)`. If rejected, falls back to the theoretical target.
fn enforce_spacing(snapped: usize, theoretical: usize, gap: f32, local_period: f32) -> (usize, bool) {
    if local_period <= 0.0 {
        return (snapped, true);
    }
    let ratio = gap / local_period;
    if ratio >= MIN_PERIOD_RATIO && ratio <= MAX_PERIOD_RATIO {
        (snapped, true)
    } else {
        (theoretical, false)
    }
}

/// Snap to the best peak of a given polarity within a window.
/// Falls back to absolute amplitude if no same-polarity candidate is found.
fn snap_to_peak_polarized(samples: &[f32], center: usize, radius: usize, positive: bool) -> usize {
    let start = center.saturating_sub(radius);
    let end = (center + radius + 1).min(samples.len());
    if start >= end {
        return center.min(samples.len().saturating_sub(1));
    }

    // Try same-polarity peaks first
    let mut best_pol_idx: Option<usize> = None;
    let mut best_pol_val: f32 = 0.0;
    for i in start..end {
        let s = samples[i];
        let matches_polarity = if positive { s >= 0.0 } else { s < 0.0 };
        if matches_polarity && s.abs() > best_pol_val {
            best_pol_val = s.abs();
            best_pol_idx = Some(i);
        }
    }

    if let Some(idx) = best_pol_idx {
        if best_pol_val > 0.0 {
            return idx;
        }
    }

    // Fallback: absolute amplitude
    snap_to_peak(samples, center, radius)
}

/// Snap to the sample with the highest absolute amplitude within a window.
fn snap_to_peak(samples: &[f32], center: usize, radius: usize) -> usize {
    let start = center.saturating_sub(radius);
    let end = (center + radius + 1).min(samples.len());
    if start >= end {
        return center.min(samples.len().saturating_sub(1));
    }

    let mut best_idx = start;
    let mut best_abs = samples[start].abs();
    for i in (start + 1)..end {
        let a = samples[i].abs();
        if a > best_abs {
            best_abs = a;
            best_idx = i;
        }
    }
    best_idx
}

/// Get the local period at a given global sample index from pitch frames.
/// This is the **primary** period source; falls back to `default_period` only
/// when no nearby voiced pitch frame is available.
fn local_period_at(
    global_sample: usize,
    default_period: f32,
    pitch_frames: Option<&[tempo_domain::PitchFrame]>,
) -> f32 {
    if let Some(frames) = pitch_frames {
        let mut best: Option<&tempo_domain::PitchFrame> = None;
        let mut best_dist = usize::MAX;
        for f in frames {
            if f.voiced && f.period_samples > 0.0 {
                let dist = if f.center_sample > global_sample {
                    f.center_sample - global_sample
                } else {
                    global_sample - f.center_sample
                };
                if dist < best_dist {
                    best_dist = dist;
                    best = Some(f);
                }
            }
        }
        if let Some(f) = best {
            return f.period_samples;
        }
    }
    default_period
}

/// Confidence based on how prominent the peak is relative to its neighborhood.
fn peak_confidence(samples: &[f32], idx: usize, radius: usize) -> f32 {
    let start = idx.saturating_sub(radius);
    let end = (idx + radius + 1).min(samples.len());
    if start >= end || idx >= samples.len() {
        return 0.0;
    }

    let peak_abs = samples[idx].abs();
    let mean_abs: f32 =
        samples[start..end].iter().map(|s| s.abs()).sum::<f32>() / (end - start) as f32;

    if mean_abs > 1e-10 {
        (peak_abs / mean_abs).min(2.0) / 2.0
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempo_domain::{
        PitchFrame, SegmentAudio, SegmentKind, SegmentPitchData, SegmentVoicedRegions,
        TempoPipelineContext, VoicedRegion,
    };

    fn sine_ctx(freq: f32, rate: u32, duration_samples: usize) -> TempoPipelineContext {
        let samples: Vec<f32> = (0..duration_samples)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin())
            .collect();
        let period = rate as f32 / freq;

        let pitch_frames: Vec<PitchFrame> = (0..10)
            .map(|i| PitchFrame {
                center_sample: i * 160 + 80,
                voiced: true,
                f0_hz: freq,
                period_samples: period,
            })
            .collect();

        let mut ctx =
            TempoPipelineContext::new(samples.clone(), rate, Vec::new(), Vec::new());
        ctx.segment_audios = vec![SegmentAudio {
            analysis_samples: samples,
            rendered_samples: Vec::new(),
            global_start_sample: 0,
            global_end_sample: duration_samples,
            extract_start_sample: 0,
            extract_end_sample: duration_samples,
            useful_start_in_analysis: 0,
            useful_end_in_analysis: duration_samples,
            target_duration_samples: duration_samples,
            alpha: 1.0,
            kind: SegmentKind::Word,
        }];
        ctx.pitch_data = vec![SegmentPitchData {
            segment_index: 0,
            frames: pitch_frames,
        }];
        ctx.voiced_regions = vec![SegmentVoicedRegions {
            segment_index: 0,
            regions: vec![VoicedRegion {
                start_sample: 0,
                end_sample: duration_samples,
                mean_f0: freq,
                mean_period_samples: period,
                stability_score: 0.95,
            }],
        }];
        ctx
    }

    #[test]
    fn generates_pitch_marks_for_sine() {
        let rate = 16_000u32;
        let freq = 200.0;
        let n = 4800;
        let mut ctx = sine_ctx(freq, rate, n);

        let stage = PitchMarkStage;
        stage.execute(&mut ctx).expect("should succeed");

        assert_eq!(ctx.pitch_marks.len(), 1);
        let marks = &ctx.pitch_marks[0].marks;
        assert!(marks.len() > 3, "should generate multiple marks");

        for w in marks.windows(2) {
            assert!(w[0].sample_index < w[1].sample_index);
        }

        let expected_period = rate as f32 / freq;
        for w in marks.windows(2) {
            let gap = w[1].sample_index as f32 - w[0].sample_index as f32;
            assert!(
                (gap - expected_period).abs() < expected_period * 0.35,
                "gap {} should be near period {}",
                gap,
                expected_period
            );
        }
    }

    #[test]
    fn marks_stay_within_region() {
        let mut ctx = sine_ctx(150.0, 16_000, 4800);
        let stage = PitchMarkStage;
        stage.execute(&mut ctx).expect("should succeed");

        for mark in &ctx.pitch_marks[0].marks {
            assert!(mark.sample_index < 4800);
        }
    }

    #[test]
    fn marks_have_consistent_polarity() {
        let rate = 16_000u32;
        let freq = 200.0;
        let n = 4800;
        let ctx_data = sine_ctx(freq, rate, n);
        let samples = &ctx_data.segment_audios[0].analysis_samples;

        let mut ctx = sine_ctx(freq, rate, n);
        let stage = PitchMarkStage;
        stage.execute(&mut ctx).expect("should succeed");

        let marks = &ctx.pitch_marks[0].marks;
        if marks.len() >= 2 {
            let first_polarity = samples[marks[0].sample_index] >= 0.0;
            let consistent_count = marks.iter()
                .filter(|m| (samples[m.sample_index] >= 0.0) == first_polarity)
                .count();
            let consistent_pct = consistent_count as f32 / marks.len() as f32;
            assert!(
                consistent_pct > 0.7,
                "at least 70% of marks should have consistent polarity, got {:.0}%",
                consistent_pct * 100.0
            );
        }
    }

    #[test]
    fn spacing_is_within_bounds() {
        let rate = 16_000u32;
        let freq = 200.0;
        let n = 4800;
        let mut ctx = sine_ctx(freq, rate, n);
        let stage = PitchMarkStage;
        stage.execute(&mut ctx).expect("should succeed");

        let marks = &ctx.pitch_marks[0].marks;
        let (_, _, out_of_range_pct) = mark_diagnostics(marks);
        assert!(
            out_of_range_pct < 30.0,
            "out-of-range spacing should be below 30%, got {:.0}%",
            out_of_range_pct
        );
    }

    #[test]
    fn rejects_empty_voiced_regions() {
        let mut ctx = TempoPipelineContext::new(vec![0.0; 100], 16_000, Vec::new(), Vec::new());
        let stage = PitchMarkStage;
        assert!(stage.execute(&mut ctx).is_err());
    }

    #[test]
    fn snap_to_peak_finds_maximum() {
        let samples = vec![0.1, 0.3, 0.8, 0.2, 0.1];
        assert_eq!(snap_to_peak(&samples, 2, 2), 2);
        assert_eq!(snap_to_peak(&samples, 0, 3), 2);
    }

    #[test]
    fn snap_polarized_prefers_same_polarity() {
        let samples = vec![-0.9, 0.3, -0.5, 0.8, -0.2];
        // Looking for positive peak near center=2 with radius=2
        let pos = snap_to_peak_polarized(&samples, 2, 2, true);
        assert_eq!(pos, 3, "should find the positive peak at index 3");

        let neg = snap_to_peak_polarized(&samples, 2, 2, false);
        assert_eq!(neg, 0, "should find the negative peak at index 0");
    }

    #[test]
    fn enforce_spacing_accepts_good_gap() {
        let (pos, accepted) = enforce_spacing(100, 100, 80.0, 80.0);
        assert!(accepted);
        assert_eq!(pos, 100);
    }

    #[test]
    fn enforce_spacing_rejects_bad_gap() {
        let (pos, accepted) = enforce_spacing(100, 90, 40.0, 80.0);
        assert!(!accepted);
        assert_eq!(pos, 90);
    }
}
