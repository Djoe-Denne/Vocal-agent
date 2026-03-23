mod wsola;

use async_trait::async_trait;
use tempo_domain::{DomainError, TempoMatchOutput, TempoMatchPort, TempoMatchRequest, WordTiming};

pub use wsola::{crossfade, rms_db, wsola_stretch};

#[derive(Debug, Clone)]
pub struct WsolaConfig {
    pub sample_rate_hz: u32,
    pub window_ms: u32,
    pub overlap_ratio: f32,
    pub crossfade_ms: u32,
    pub stretch_tolerance: f32,
    pub max_stretch_ratio: f32,
    pub min_stretch_ratio: f32,
    pub silence_threshold_db: f32,
}

impl Default for WsolaConfig {
    fn default() -> Self {
        Self {
            sample_rate_hz: 16_000,
            window_ms: 30,
            overlap_ratio: 0.75,
            crossfade_ms: 8,
            stretch_tolerance: 0.05,
            max_stretch_ratio: 6.0,
            min_stretch_ratio: 0.15,
            silence_threshold_db: -40.0,
        }
    }
}

pub struct WsolaTempoMatcher {
    config: WsolaConfig,
}

impl WsolaTempoMatcher {
    pub fn new(config: WsolaConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl TempoMatchPort for WsolaTempoMatcher {
    async fn match_tempo(
        &self,
        request: TempoMatchRequest,
    ) -> Result<TempoMatchOutput, DomainError> {
        let config = WsolaConfig {
            sample_rate_hz: request.tts_sample_rate_hz,
            ..self.config.clone()
        };

        let segments = build_segment_plan(
            &request.original_timings,
            &request.tts_timings,
            &request.tts_samples,
            &config,
        )?;

        let crossfade_samples = ms_to_samples(config.crossfade_ms, config.sample_rate_hz);
        let mut output: Vec<f32> = Vec::new();

        for segment in &segments {
            let stretched = wsola_stretch(&segment.samples, segment.stretch_ratio, &config);
            if output.is_empty() {
                output = stretched;
            } else {
                output = crossfade(&output, &stretched, crossfade_samples);
            }
        }

        tracing::debug!(
            input_samples = request.tts_samples.len(),
            output_samples = output.len(),
            segment_count = segments.len(),
            "tempo matching complete"
        );

        Ok(TempoMatchOutput {
            samples: output,
            sample_rate_hz: request.tts_sample_rate_hz,
        })
    }
}

struct AudioSegment {
    samples: Vec<f32>,
    stretch_ratio: f64,
}

fn build_segment_plan(
    original: &[WordTiming],
    tts: &[WordTiming],
    tts_samples: &[f32],
    config: &WsolaConfig,
) -> Result<Vec<AudioSegment>, DomainError> {
    if original.len() != tts.len() {
        tracing::warn!(
            original_count = original.len(),
            tts_count = tts.len(),
            "word count mismatch — falling back to global ratio stretching"
        );
        return Ok(build_global_stretch(original, tts, tts_samples, config));
    }

    let mut segments = Vec::new();
    let word_count = original.len();

    for i in 0..word_count {
        // Inter-word gap before this word (silence between previous word end and this word start)
        let tts_gap_start_ms = if i == 0 { 0 } else { tts[i - 1].end_ms };
        let tts_gap_end_ms = tts[i].start_ms;
        let orig_gap_start_ms = if i == 0 { 0 } else { original[i - 1].end_ms };
        let orig_gap_end_ms = original[i].start_ms;

        if tts_gap_end_ms > tts_gap_start_ms {
            let gap_samples = extract_samples(
                tts_samples,
                tts_gap_start_ms,
                tts_gap_end_ms,
                config.sample_rate_hz,
            );
            let tts_gap_dur = (tts_gap_end_ms - tts_gap_start_ms) as f64;
            let orig_gap_dur = if orig_gap_end_ms > orig_gap_start_ms {
                (orig_gap_end_ms - orig_gap_start_ms) as f64
            } else {
                0.0
            };

            let ratio = if tts_gap_dur > 0.0 {
                orig_gap_dur / tts_gap_dur
            } else {
                1.0
            };

            let is_silence = rms_db(&gap_samples) < config.silence_threshold_db;
            let needs_extreme_stretch =
                ratio > f64::from(config.max_stretch_ratio);

            if is_silence || needs_extreme_stretch {
                segments.push(build_silence_segment(
                    orig_gap_dur,
                    tts_gap_dur,
                    &gap_samples,
                    config,
                ));
            } else {
                segments.push(AudioSegment {
                    samples: gap_samples,
                    stretch_ratio: ratio,
                });
            }
        }

        // Word segment
        let word_samples = extract_samples(
            tts_samples,
            tts[i].start_ms,
            tts[i].end_ms,
            config.sample_rate_hz,
        );
        let tts_word_dur = (tts[i].end_ms - tts[i].start_ms) as f64;
        let orig_word_dur = (original[i].end_ms - original[i].start_ms) as f64;
        let ratio = if tts_word_dur > 0.0 {
            orig_word_dur / tts_word_dur
        } else {
            1.0
        };

        tracing::debug!(
            word = %tts[i].word,
            orig_ms = format!("{}-{}", original[i].start_ms, original[i].end_ms),
            tts_ms = format!("{}-{}", tts[i].start_ms, tts[i].end_ms),
            ratio = format!("{ratio:.2}"),
            samples = word_samples.len(),
            "segment plan: word"
        );

        segments.push(AudioSegment {
            samples: word_samples,
            stretch_ratio: ratio,
        });
    }

    // Trailing silence after last word
    if let Some(last_tts) = tts.last() {
        let total_tts_ms =
            (tts_samples.len() as u64 * 1000) / config.sample_rate_hz as u64;
        if last_tts.end_ms < total_tts_ms {
            let trailing = extract_samples(
                tts_samples,
                last_tts.end_ms,
                total_tts_ms,
                config.sample_rate_hz,
            );
            if !trailing.is_empty() {
                let orig_trailing_dur = original
                    .last()
                    .map(|last_orig| {
                        let orig_total_ms = original
                            .iter()
                            .map(|w| w.end_ms)
                            .max()
                            .unwrap_or(last_orig.end_ms);
                        if orig_total_ms > last_orig.end_ms {
                            (orig_total_ms - last_orig.end_ms) as f64
                        } else {
                            0.0
                        }
                    })
                    .unwrap_or(0.0);
                let tts_trailing_dur = (total_tts_ms - last_tts.end_ms) as f64;
                let ratio = if tts_trailing_dur > 0.0 && orig_trailing_dur > 0.0 {
                    orig_trailing_dur / tts_trailing_dur
                } else {
                    1.0
                };
                segments.push(AudioSegment {
                    samples: trailing,
                    stretch_ratio: ratio,
                });
            }
        }
    }

    Ok(segments)
}

fn build_global_stretch(
    original: &[WordTiming],
    tts: &[WordTiming],
    tts_samples: &[f32],
    _config: &WsolaConfig,
) -> Vec<AudioSegment> {
    let orig_dur = original
        .iter()
        .map(|w| w.end_ms)
        .max()
        .unwrap_or(0)
        .saturating_sub(original.iter().map(|w| w.start_ms).min().unwrap_or(0))
        as f64;
    let tts_dur = tts
        .iter()
        .map(|w| w.end_ms)
        .max()
        .unwrap_or(0)
        .saturating_sub(tts.iter().map(|w| w.start_ms).min().unwrap_or(0))
        as f64;
    let ratio = if tts_dur > 0.0 { orig_dur / tts_dur } else { 1.0 };

    vec![AudioSegment {
        samples: tts_samples.to_vec(),
        stretch_ratio: ratio,
    }]
}

fn build_silence_segment(
    orig_dur_ms: f64,
    tts_dur_ms: f64,
    gap_samples: &[f32],
    config: &WsolaConfig,
) -> AudioSegment {
    let orig_samples = ms_to_samples_f64(orig_dur_ms, config.sample_rate_hz);
    let tts_sample_count = gap_samples.len();

    if orig_samples >= tts_sample_count {
        let pad_count = orig_samples - tts_sample_count;
        let mut samples = gap_samples.to_vec();
        // Fade out existing, insert silence, fade in
        let fade_len = ms_to_samples(config.crossfade_ms, config.sample_rate_hz)
            .min(tts_sample_count / 2);
        if fade_len > 0 && samples.len() > fade_len {
            let mid = samples.len() / 2;
            for i in 0..fade_len {
                let w = i as f32 / fade_len as f32;
                samples[mid - fade_len + i] *= 1.0 - w;
            }
            for i in 0..fade_len.min(samples.len() - mid) {
                let w = i as f32 / fade_len as f32;
                samples[mid + i] *= w;
            }
        }
        let mut out = Vec::with_capacity(orig_samples);
        let half = samples.len() / 2;
        out.extend_from_slice(&samples[..half]);
        out.resize(out.len() + pad_count, 0.0);
        out.extend_from_slice(&samples[half..]);
        AudioSegment {
            samples: out,
            stretch_ratio: 1.0,
        }
    } else {
        let ratio = if tts_dur_ms > 0.0 {
            orig_dur_ms / tts_dur_ms
        } else {
            1.0
        };
        AudioSegment {
            samples: gap_samples.to_vec(),
            stretch_ratio: ratio,
        }
    }
}

fn extract_samples(
    samples: &[f32],
    start_ms: u64,
    end_ms: u64,
    sample_rate_hz: u32,
) -> Vec<f32> {
    let start_idx = ms_to_samples(start_ms as u32, sample_rate_hz).min(samples.len());
    let end_idx = ms_to_samples(end_ms as u32, sample_rate_hz).min(samples.len());
    if start_idx >= end_idx {
        return Vec::new();
    }
    samples[start_idx..end_idx].to_vec()
}

fn ms_to_samples(ms: u32, sample_rate_hz: u32) -> usize {
    (sample_rate_hz as usize * ms as usize) / 1000
}

fn ms_to_samples_f64(ms: f64, sample_rate_hz: u32) -> usize {
    (sample_rate_hz as f64 * ms / 1000.0).round() as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_timings(words: &[(&str, u64, u64)]) -> Vec<WordTiming> {
        words
            .iter()
            .map(|(word, start, end)| WordTiming {
                word: word.to_string(),
                start_ms: *start,
                end_ms: *end,
                confidence: 0.95,
            })
            .collect()
    }

    #[tokio::test]
    async fn match_tempo_preserves_sample_rate() {
        let config = WsolaConfig::default();
        let matcher = WsolaTempoMatcher::new(config);
        let sr = 16000u32;
        let samples: Vec<f32> = (0..sr as usize).map(|i| (i as f32 / sr as f32).sin()).collect();
        let original = make_timings(&[("hello", 0, 500), ("world", 600, 1000)]);
        let tts = make_timings(&[("hello", 0, 400), ("world", 450, 900)]);

        let output = matcher
            .match_tempo(TempoMatchRequest {
                tts_samples: samples,
                tts_sample_rate_hz: sr,
                original_timings: original,
                tts_timings: tts,
            })
            .await
            .expect("match_tempo should succeed");

        assert_eq!(output.sample_rate_hz, sr);
        assert!(!output.samples.is_empty());
    }

    #[tokio::test]
    async fn match_tempo_handles_count_mismatch() {
        let config = WsolaConfig::default();
        let matcher = WsolaTempoMatcher::new(config);
        let sr = 16000u32;
        let samples: Vec<f32> = vec![0.0; sr as usize];
        let original = make_timings(&[("hello", 0, 500)]);
        let tts = make_timings(&[("hello", 0, 400), ("world", 450, 900)]);

        let output = matcher
            .match_tempo(TempoMatchRequest {
                tts_samples: samples,
                tts_sample_rate_hz: sr,
                original_timings: original,
                tts_timings: tts,
            })
            .await
            .expect("match_tempo should succeed with fallback");

        assert!(!output.samples.is_empty());
    }
}
