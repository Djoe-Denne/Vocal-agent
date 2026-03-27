use tempo_domain::{
    DomainError, PitchFrame, SegmentKind, SegmentPitchData, TempoPipelineContext,
    TempoPipelineStage,
};

const MIN_F0_HZ: f32 = 60.0;
const MAX_F0_HZ: f32 = 350.0;
const MEDIAN_WINDOW: usize = 5;
const AUTOCORR_VOICED_THRESHOLD: f32 = 0.3;

/// Step 5: estimate F0 on each voiced frame via autocorrelation.
///
/// Gap segments are skipped (empty pitch data pushed to keep indexing aligned).
pub struct F0EstimationStage;

impl TempoPipelineStage for F0EstimationStage {
    fn name(&self) -> &'static str {
        "f0_estimation"
    }

    fn execute(&self, context: &mut TempoPipelineContext) -> Result<(), DomainError> {
        if context.frame_analyses.is_empty() {
            return Err(DomainError::internal_error(
                "f0_estimation: no frame analyses available",
            ));
        }
        if context.segment_audios.len() != context.frame_analyses.len() {
            return Err(DomainError::internal_error(
                "f0_estimation: segment_audios and frame_analyses count mismatch",
            ));
        }

        let rate = context.sample_rate_hz;
        let mut all_pitch_data = Vec::with_capacity(context.frame_analyses.len());

        for (seg_idx, analysis) in context.frame_analyses.iter().enumerate() {
            if context.segment_audios[seg_idx].kind == SegmentKind::Gap {
                all_pitch_data.push(SegmentPitchData {
                    segment_index: seg_idx,
                    frames: Vec::new(),
                });
                continue;
            }

            let samples = &context.segment_audios[seg_idx].analysis_samples;
            let hop = analysis.hop_samples;
            let frame_len = analysis.frame_length_samples;

            let mut raw_frames: Vec<PitchFrame> = Vec::with_capacity(analysis.frames.len());

            for (frame_idx, metrics) in analysis.frames.iter().enumerate() {
                let offset = frame_idx * hop;
                let center_sample = offset + frame_len / 2;

                if !metrics.is_voiced {
                    raw_frames.push(PitchFrame {
                        center_sample,
                        voiced: false,
                        f0_hz: 0.0,
                        period_samples: 0.0,
                    });
                    continue;
                }

                let end = (offset + frame_len).min(samples.len());
                let frame_slice = &samples[offset..end];

                let (period, strength) = estimate_period_autocorr(frame_slice, rate);

                if strength < AUTOCORR_VOICED_THRESHOLD || period <= 0.0 {
                    raw_frames.push(PitchFrame {
                        center_sample,
                        voiced: false,
                        f0_hz: 0.0,
                        period_samples: 0.0,
                    });
                } else {
                    let f0 = rate as f32 / period;
                    raw_frames.push(PitchFrame {
                        center_sample,
                        voiced: true,
                        f0_hz: f0,
                        period_samples: period,
                    });
                }
            }

            median_smooth_f0(&mut raw_frames);
            let octave_corrections = octave_correct_f0(&mut raw_frames, rate);

            let voiced_count = raw_frames.iter().filter(|f| f.voiced).count();
            tracing::debug!(
                segment_index = seg_idx,
                total_frames = raw_frames.len(),
                voiced_count,
                octave_corrections,
                "F0 estimation complete for segment"
            );

            all_pitch_data.push(SegmentPitchData {
                segment_index: seg_idx,
                frames: raw_frames,
            });
        }

        tracing::debug!(
            segment_count = all_pitch_data.len(),
            "F0 estimation complete"
        );

        context.pitch_data = all_pitch_data;
        Ok(())
    }
}

fn estimate_period_autocorr(frame: &[f32], sample_rate_hz: u32) -> (f32, f32) {
    let n = frame.len();
    if n < 4 || sample_rate_hz == 0 {
        return (0.0, 0.0);
    }

    let min_lag = (sample_rate_hz as f32 / MAX_F0_HZ).ceil() as usize;
    let max_lag = ((sample_rate_hz as f32 / MIN_F0_HZ).floor() as usize).min(n - 1);

    if min_lag >= max_lag || max_lag >= n {
        return (0.0, 0.0);
    }

    let energy: f32 = frame.iter().map(|s| s * s).sum();
    if energy < 1e-10 {
        return (0.0, 0.0);
    }

    let mut r_values: Vec<f32> = Vec::with_capacity(max_lag - min_lag + 1);
    for lag in min_lag..=max_lag {
        let mut num = 0.0f32;
        let mut den_a = 0.0f32;
        let mut den_b = 0.0f32;
        for i in 0..(n - lag) {
            num += frame[i] * frame[i + lag];
            den_a += frame[i] * frame[i];
            den_b += frame[i + lag] * frame[i + lag];
        }
        let den = (den_a * den_b).sqrt();
        let r = if den > 1e-10 { num / den } else { 0.0 };
        r_values.push(r);
    }

    let threshold = AUTOCORR_VOICED_THRESHOLD;
    for i in 1..r_values.len().saturating_sub(1) {
        if r_values[i] >= threshold && r_values[i] >= r_values[i - 1] && r_values[i] >= r_values[i + 1] {
            let lag = min_lag + i;
            return (lag as f32, r_values[i].clamp(0.0, 1.0));
        }
    }

    let (best_i, &best_r) = r_values
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or((0, &0.0));

    if best_r < threshold {
        return (0.0, 0.0);
    }

    let lag = min_lag + best_i;
    (lag as f32, best_r.clamp(0.0, 1.0))
}

fn median_smooth_f0(frames: &mut [PitchFrame]) {
    if frames.len() < MEDIAN_WINDOW {
        return;
    }

    let original_f0: Vec<f32> = frames.iter().map(|f| f.f0_hz).collect();
    let half = MEDIAN_WINDOW / 2;

    for i in half..(frames.len() - half) {
        if !frames[i].voiced {
            continue;
        }

        let mut window: Vec<f32> = Vec::with_capacity(MEDIAN_WINDOW);
        for j in (i - half)..=(i + half) {
            if frames[j].voiced {
                window.push(original_f0[j]);
            }
        }

        if window.len() >= 3 {
            window.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let median = window[window.len() / 2];
            frames[i].f0_hz = median;
            if median > 0.0 {
                frames[i].period_samples =
                    original_f0[i] / median * frames[i].period_samples;
            }
        }
    }

    for frame in frames.iter_mut() {
        if frame.voiced && frame.f0_hz > 0.0 {
            frame.period_samples = frame.period_samples.round();
            if frame.period_samples <= 0.0 {
                frame.voiced = false;
                frame.f0_hz = 0.0;
            }
        }
    }
}

/// Correct octave jumps by comparing each voiced frame with the previous one.
/// Returns the number of corrections applied.
fn octave_correct_f0(frames: &mut [PitchFrame], sample_rate_hz: u32) -> usize {
    const OCTAVE_JUMP_RATIO: f32 = 1.8;

    let mut corrections = 0usize;
    let mut prev_f0: Option<f32> = None;

    for frame in frames.iter_mut() {
        if !frame.voiced || frame.f0_hz <= 0.0 {
            continue;
        }

        if let Some(pf0) = prev_f0 {
            if frame.f0_hz > OCTAVE_JUMP_RATIO * pf0 {
                let half = frame.f0_hz / 2.0;
                if half >= MIN_F0_HZ && half <= MAX_F0_HZ
                    && (half - pf0).abs() < (frame.f0_hz - pf0).abs()
                {
                    frame.f0_hz = half;
                    frame.period_samples = (sample_rate_hz as f32 / half).round();
                    corrections += 1;
                }
            } else if frame.f0_hz < pf0 / OCTAVE_JUMP_RATIO {
                let double = frame.f0_hz * 2.0;
                if double >= MIN_F0_HZ && double <= MAX_F0_HZ
                    && (double - pf0).abs() < (frame.f0_hz - pf0).abs()
                {
                    frame.f0_hz = double;
                    frame.period_samples = (sample_rate_hz as f32 / double).round();
                    corrections += 1;
                }
            }
        }

        prev_f0 = Some(frame.f0_hz);
    }

    corrections
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempo_domain::{FrameMetrics, SegmentAudio, SegmentFrameAnalysis, SegmentKind, TempoPipelineContext};

    fn sine_samples(freq: f32, rate: u32, duration_ms: u64) -> Vec<f32> {
        let n = (rate as u64 * duration_ms / 1000) as usize;
        (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin())
            .collect()
    }

    fn build_ctx(samples: Vec<f32>, rate: u32) -> TempoPipelineContext {
        let frame_len = (rate as f64 * 0.030) as usize;
        let hop = (rate as f64 * 0.010) as usize;
        let n = samples.len();

        let mut frames = Vec::new();
        let mut offset = 0;
        while offset + frame_len <= n {
            let frame_slice = &samples[offset..offset + frame_len];
            let energy: f32 =
                (frame_slice.iter().map(|s| s * s).sum::<f32>() / frame_slice.len() as f32).sqrt();
            frames.push(FrameMetrics {
                energy,
                is_voiced: energy > 1e-4,
                periodicity: 0.8,
            });
            offset += hop;
        }

        let mut ctx = TempoPipelineContext::new(samples.clone(), rate, Vec::new(), Vec::new());
        ctx.segment_audios = vec![SegmentAudio {
            analysis_samples: samples,
            rendered_samples: Vec::new(),
            global_start_sample: 0,
            global_end_sample: n,
            extract_start_sample: 0,
            extract_end_sample: n,
            useful_start_in_analysis: 0,
            useful_end_in_analysis: n,
            target_duration_samples: n,
            alpha: 1.0,
            kind: SegmentKind::Word,
        }];
        ctx.frame_analyses = vec![SegmentFrameAnalysis {
            segment_index: 0,
            frame_length_samples: frame_len,
            hop_samples: hop,
            frames,
        }];
        ctx
    }

    #[test]
    fn estimates_f0_for_pure_tone() {
        let rate = 16_000u32;
        let freq = 200.0;
        let samples = sine_samples(freq, rate, 300);
        let mut ctx = build_ctx(samples, rate);

        let stage = F0EstimationStage;
        stage.execute(&mut ctx).expect("should succeed");

        assert_eq!(ctx.pitch_data.len(), 1);
        let voiced: Vec<&PitchFrame> =
            ctx.pitch_data[0].frames.iter().filter(|f| f.voiced).collect();
        assert!(!voiced.is_empty(), "should detect voiced frames");

        let avg_f0 = voiced.iter().map(|f| f.f0_hz).sum::<f32>() / voiced.len() as f32;
        assert!(
            (avg_f0 - freq).abs() < 30.0,
            "avg F0 {avg_f0} should be near {freq}"
        );
    }

    #[test]
    fn silent_signal_produces_unvoiced_frames() {
        let samples = vec![0.0; 4800];
        let mut ctx = build_ctx(samples, 16_000);
        for f in &mut ctx.frame_analyses[0].frames {
            f.is_voiced = false;
        }

        let stage = F0EstimationStage;
        stage.execute(&mut ctx).expect("should succeed");

        for frame in &ctx.pitch_data[0].frames {
            assert!(!frame.voiced);
            assert_eq!(frame.f0_hz, 0.0);
        }
    }

    #[test]
    fn rejects_empty_frame_analyses() {
        let mut ctx = TempoPipelineContext::new(vec![0.0; 100], 16_000, Vec::new(), Vec::new());
        let stage = F0EstimationStage;
        assert!(stage.execute(&mut ctx).is_err());
    }

    #[test]
    fn octave_jump_is_corrected() {
        let rate = 16_000u32;
        let mut frames = vec![
            PitchFrame { center_sample: 0, voiced: true, f0_hz: 200.0, period_samples: 80.0 },
            PitchFrame { center_sample: 160, voiced: true, f0_hz: 195.0, period_samples: 82.0 },
            PitchFrame { center_sample: 320, voiced: true, f0_hz: 400.0, period_samples: 40.0 }, // octave up
            PitchFrame { center_sample: 480, voiced: true, f0_hz: 198.0, period_samples: 81.0 },
        ];
        let corrections = octave_correct_f0(&mut frames, rate);
        assert!(corrections > 0, "should have corrected at least one octave jump");
        assert!(
            (frames[2].f0_hz - 200.0).abs() < 10.0,
            "corrected F0 {} should be near 200",
            frames[2].f0_hz
        );
    }

    #[test]
    fn octave_drop_is_corrected() {
        let rate = 16_000u32;
        let mut frames = vec![
            PitchFrame { center_sample: 0, voiced: true, f0_hz: 200.0, period_samples: 80.0 },
            PitchFrame { center_sample: 160, voiced: true, f0_hz: 100.0, period_samples: 160.0 }, // octave down
            PitchFrame { center_sample: 320, voiced: true, f0_hz: 205.0, period_samples: 78.0 },
        ];
        let corrections = octave_correct_f0(&mut frames, rate);
        assert!(corrections > 0);
        assert!(
            (frames[1].f0_hz - 200.0).abs() < 10.0,
            "corrected F0 {} should be near 200",
            frames[1].f0_hz
        );
    }

    #[test]
    fn autocorr_returns_correct_period_for_sine() {
        let rate = 16_000u32;
        let freq = 200.0f32;
        let n = 480;
        let frame: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin())
            .collect();

        let (period, strength) = estimate_period_autocorr(&frame, rate);
        let expected_period = rate as f32 / freq;
        assert!(
            (period - expected_period).abs() < 2.0,
            "period {period} should be near {expected_period}"
        );
        assert!(strength > 0.5, "strength {strength} should be high");
    }
}
