use tempo_domain::{
    DomainError, FrameMetrics, SegmentFrameAnalysis, SegmentKind, TempoPipelineContext,
    TempoPipelineStage,
};

const DEFAULT_FRAME_MS: u64 = 30;
const DEFAULT_HOP_MS: u64 = 10;
const ENERGY_FLOOR: f32 = 1e-10;
const ZCR_VOICED_THRESHOLD: f32 = 0.15;
const ENERGY_SILENCE_THRESHOLD: f32 = 1e-6;
const PERIODICITY_VOICED_THRESHOLD: f32 = 0.4;

/// Step 4: frame-by-frame analysis of energy, voicing, and periodicity.
///
/// For each extracted segment, slides a window over the analysis samples and
/// computes per-frame metrics used by later synthesis stages.
/// Gap segments are skipped (empty analysis pushed to keep indexing aligned).
pub struct FrameAnalysisStage {
    frame_length_ms: u64,
    hop_ms: u64,
}

impl FrameAnalysisStage {
    pub fn new(frame_length_ms: u64, hop_ms: u64) -> Self {
        Self {
            frame_length_ms,
            hop_ms,
        }
    }
}

impl Default for FrameAnalysisStage {
    fn default() -> Self {
        Self::new(DEFAULT_FRAME_MS, DEFAULT_HOP_MS)
    }
}

impl TempoPipelineStage for FrameAnalysisStage {
    fn name(&self) -> &'static str {
        "frame_analysis"
    }

    fn execute(&self, context: &mut TempoPipelineContext) -> Result<(), DomainError> {
        if context.segment_audios.is_empty() {
            return Err(DomainError::internal_error(
                "frame_analysis: no segment audios available",
            ));
        }

        let rate = context.sample_rate_hz;
        let frame_len =
            tempo_domain::convert::ms_to_samples(self.frame_length_ms, rate).max(1);
        let hop = tempo_domain::convert::ms_to_samples(self.hop_ms, rate).max(1);

        let mut analyses = Vec::with_capacity(context.segment_audios.len());

        for (idx, seg) in context.segment_audios.iter().enumerate() {
            if seg.kind == SegmentKind::Gap {
                analyses.push(SegmentFrameAnalysis {
                    segment_index: idx,
                    frame_length_samples: frame_len,
                    hop_samples: hop,
                    frames: Vec::new(),
                });
                continue;
            }

            let samples = &seg.analysis_samples;
            let mut frames = Vec::new();

            let mut offset = 0usize;
            while offset + frame_len <= samples.len() {
                let frame = &samples[offset..offset + frame_len];
                let energy = rms_energy(frame);
                let zcr = zero_crossing_rate(frame);
                let periodicity = normalized_autocorrelation_peak(frame, rate);

                let is_voiced = energy > ENERGY_SILENCE_THRESHOLD
                    && zcr < ZCR_VOICED_THRESHOLD
                    && periodicity > PERIODICITY_VOICED_THRESHOLD;

                frames.push(FrameMetrics {
                    energy,
                    is_voiced,
                    periodicity,
                });

                offset += hop;
            }

            tracing::trace!(
                segment_index = idx,
                frame_count = frames.len(),
                voiced_count = frames.iter().filter(|f| f.is_voiced).count(),
                "segment frame analysis complete"
            );

            analyses.push(SegmentFrameAnalysis {
                segment_index: idx,
                frame_length_samples: frame_len,
                hop_samples: hop,
                frames,
            });
        }

        tracing::debug!(
            segment_count = analyses.len(),
            frame_length_ms = self.frame_length_ms,
            hop_ms = self.hop_ms,
            "frame analysis complete"
        );

        context.frame_analyses = analyses;
        Ok(())
    }
}

fn rms_energy(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let mean_sq = frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32;
    mean_sq.sqrt()
}

fn zero_crossing_rate(frame: &[f32]) -> f32 {
    if frame.len() < 2 {
        return 0.0;
    }
    let crossings = frame
        .windows(2)
        .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
        .count();
    crossings as f32 / (frame.len() - 1) as f32
}

/// Estimate periodicity via normalized autocorrelation.
/// Returns the peak value in the plausible pitch range (50-500 Hz).
fn normalized_autocorrelation_peak(frame: &[f32], sample_rate_hz: u32) -> f32 {
    let n = frame.len();
    if n < 4 || sample_rate_hz == 0 {
        return 0.0;
    }

    let min_lag = (sample_rate_hz as usize) / 500;
    let max_lag = ((sample_rate_hz as usize) / 50).min(n - 1);

    if min_lag >= max_lag || max_lag >= n {
        return 0.0;
    }

    let energy: f32 = frame.iter().map(|s| s * s).sum();
    if energy < ENERGY_FLOOR {
        return 0.0;
    }

    let mut best = 0.0f32;

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
        if den > ENERGY_FLOOR {
            let r = num / den;
            if r > best {
                best = r;
            }
        }
    }

    best.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempo_domain::{SegmentAudio, SegmentKind, TempoPipelineContext};

    fn ctx_with_segment(samples: Vec<f32>, rate: u32) -> TempoPipelineContext {
        let n = samples.len();
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
        ctx
    }

    #[test]
    fn produces_frames_for_valid_segment() {
        let rate = 16_000u32;
        let samples = vec![0.5; 4800];
        let mut ctx = ctx_with_segment(samples, rate);
        let stage = FrameAnalysisStage::default();
        stage.execute(&mut ctx).expect("should succeed");

        assert_eq!(ctx.frame_analyses.len(), 1);
        let analysis = &ctx.frame_analyses[0];
        assert!(analysis.frames.len() > 1);
        assert_eq!(analysis.frame_length_samples, 480);
        assert_eq!(analysis.hop_samples, 160);
    }

    #[test]
    fn silent_frames_are_unvoiced() {
        let samples = vec![0.0; 4800];
        let mut ctx = ctx_with_segment(samples, 16_000);
        let stage = FrameAnalysisStage::default();
        stage.execute(&mut ctx).expect("should succeed");

        for frame in &ctx.frame_analyses[0].frames {
            assert!(!frame.is_voiced);
        }
    }

    #[test]
    fn periodic_signal_has_high_periodicity() {
        let rate = 16_000u32;
        let freq = 200.0f32;
        let samples: Vec<f32> = (0..4800)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin())
            .collect();
        let mut ctx = ctx_with_segment(samples, rate);
        let stage = FrameAnalysisStage::default();
        stage.execute(&mut ctx).expect("should succeed");

        let avg_periodicity: f32 = ctx.frame_analyses[0]
            .frames
            .iter()
            .map(|f| f.periodicity)
            .sum::<f32>()
            / ctx.frame_analyses[0].frames.len() as f32;
        assert!(
            avg_periodicity > 0.7,
            "expected high periodicity for pure tone, got {}",
            avg_periodicity
        );
    }

    #[test]
    fn rejects_empty_segment_audios() {
        let mut ctx = TempoPipelineContext::new(vec![0.0; 100], 16_000, Vec::new(), Vec::new());
        let stage = FrameAnalysisStage::default();
        assert!(stage.execute(&mut ctx).is_err());
    }

    #[test]
    fn gap_segment_produces_empty_analysis() {
        let mut ctx = TempoPipelineContext::new(vec![0.0; 1600], 16_000, Vec::new(), Vec::new());
        ctx.segment_audios = vec![SegmentAudio {
            analysis_samples: vec![0.0; 800],
            rendered_samples: vec![0.0; 800],
            global_start_sample: 0,
            global_end_sample: 800,
            extract_start_sample: 0,
            extract_end_sample: 800,
            useful_start_in_analysis: 0,
            useful_end_in_analysis: 800,
            target_duration_samples: 800,
            alpha: 1.0,
            kind: SegmentKind::Gap,
        }];
        let stage = FrameAnalysisStage::default();
        stage.execute(&mut ctx).expect("should succeed");
        assert!(ctx.frame_analyses[0].frames.is_empty());
    }

    #[test]
    fn rms_energy_returns_positive() {
        let frame = vec![0.5, -0.5, 0.3, -0.3];
        let e = rms_energy(&frame);
        assert!(e > 0.0);
    }

    #[test]
    fn zcr_of_alternating_signal_is_high() {
        let frame: Vec<f32> = (0..100).map(|i| if i % 2 == 0 { 1.0 } else { -1.0 }).collect();
        let zcr = zero_crossing_rate(&frame);
        assert!((zcr - 1.0).abs() < 0.02);
    }

    #[test]
    fn zcr_of_constant_signal_is_zero() {
        let frame = vec![0.5; 100];
        let zcr = zero_crossing_rate(&frame);
        assert_eq!(zcr, 0.0);
    }

    #[test]
    fn noisy_signal_with_low_periodicity_is_unvoiced() {
        let rate = 16_000u32;
        // Pseudo-random noise via LCG -- high energy, zero-mean, no periodicity
        let mut rng: u32 = 12345;
        let samples: Vec<f32> = (0..4800)
            .map(|_| {
                rng = rng.wrapping_mul(1103515245).wrapping_add(12345);
                let val = ((rng >> 16) as i16) as f32 / i16::MAX as f32;
                val * 0.6
            })
            .collect();
        let mut ctx = ctx_with_segment(samples, rate);
        let stage = FrameAnalysisStage::default();
        stage.execute(&mut ctx).expect("should succeed");

        let voiced_count = ctx.frame_analyses[0].frames.iter().filter(|f| f.is_voiced).count();
        let total = ctx.frame_analyses[0].frames.len();
        let voiced_pct = if total > 0 { voiced_count as f32 / total as f32 } else { 0.0 };
        assert!(
            voiced_pct < 0.3,
            "noisy signal should have few voiced frames, got {:.0}%",
            voiced_pct * 100.0
        );
    }
}
