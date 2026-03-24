use tempo_domain::{
    DomainError, PitchMark, SegmentPitchMarks, TempoPipelineContext, TempoPipelineStage,
};

const SEARCH_WINDOW_RATIO: f32 = 0.25;

/// Step 7: generate pitch marks within voiced regions.
///
/// For each voiced region, picks a credible starting point (peak amplitude near
/// the region center), then propagates left and right using local T0 period,
/// snapping each mark to the nearest amplitude peak within a search window.
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
            let samples = &context.segment_audios[seg_idx].local_samples;
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

            tracing::trace!(
                segment_index = seg_idx,
                mark_count = segment_marks.len(),
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

    let period_i = mean_period.round() as usize;
    let search_radius = ((mean_period * SEARCH_WINDOW_RATIO).round() as usize).max(1);

    // Find the best starting point near the center of the region
    let center = n / 2;
    let seed_local = snap_to_peak(region_samples, center, search_radius.max(period_i / 2));
    let seed_period = local_period_at(seed_local + global_offset, mean_period, pitch_frames);

    let mut marks = Vec::new();
    marks.push(PitchMark {
        sample_index: seed_local + global_offset,
        local_period_samples: seed_period,
        confidence: peak_confidence(region_samples, seed_local, search_radius),
    });

    // Propagate right from seed
    let mut pos = seed_local as f64 + seed_period as f64;
    while (pos.round() as usize) < n {
        let target = pos.round() as usize;
        let snapped = snap_to_peak(region_samples, target, search_radius);
        let lp = local_period_at(snapped + global_offset, mean_period, pitch_frames);
        marks.push(PitchMark {
            sample_index: snapped + global_offset,
            local_period_samples: lp,
            confidence: peak_confidence(region_samples, snapped, search_radius),
        });
        pos = snapped as f64 + lp as f64;
    }

    // Propagate left from seed
    let mut pos = seed_local as f64 - seed_period as f64;
    while pos >= 0.0 {
        let target = pos.round() as usize;
        let snapped = snap_to_peak(region_samples, target, search_radius);
        let lp = local_period_at(snapped + global_offset, mean_period, pitch_frames);
        marks.push(PitchMark {
            sample_index: snapped + global_offset,
            local_period_samples: lp,
            confidence: peak_confidence(region_samples, snapped, search_radius),
        });
        pos = snapped as f64 - lp as f64;
    }

    marks.sort_by_key(|m| m.sample_index);
    marks.dedup_by_key(|m| m.sample_index);
    marks
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

/// Get the local period at a given global sample index from pitch frames,
/// falling back to mean_period if no match is found.
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
        PitchFrame, SegmentAudio, SegmentPitchData, SegmentVoicedRegions, TempoPipelineContext,
        VoicedRegion,
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
            local_samples: samples,
            global_start_sample: 0,
            global_end_sample: duration_samples,
            margin_left: 0,
            margin_right: 0,
            target_duration_samples: duration_samples,
            alpha: 1.0,
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
        let n = 4800; // 300ms
        let mut ctx = sine_ctx(freq, rate, n);

        let stage = PitchMarkStage;
        stage.execute(&mut ctx).expect("should succeed");

        assert_eq!(ctx.pitch_marks.len(), 1);
        let marks = &ctx.pitch_marks[0].marks;
        assert!(marks.len() > 3, "should generate multiple marks");

        // Marks should be sorted
        for w in marks.windows(2) {
            assert!(w[0].sample_index < w[1].sample_index);
        }

        // Spacing should be roughly one period (~80 samples for 200Hz at 16kHz)
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
}
