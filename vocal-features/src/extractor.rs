use crate::{
    energy::{rms_energy, rms_energy_frames},
    types::{FrameMeasurement, ProsodyFeatures, SegmentAnalysis, WordBoundary, WordFeatures},
    util::ms_to_samples,
    yin::{estimate_f0, YinConfig},
};

pub struct FeatureExtractorConfig {
    pub sample_rate: u32,
    pub f0_min_hz: f32,
    pub f0_max_hz: f32,
    pub voicing_threshold: f32,
}

impl Default for FeatureExtractorConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16_000,
            f0_min_hz: 50.0,
            f0_max_hz: 500.0,
            voicing_threshold: 0.3,
        }
    }
}

pub struct FeatureExtractor {
    yin_config: YinConfig,
    sample_rate: u32,
}

impl FeatureExtractor {
    pub fn new(config: FeatureExtractorConfig) -> Self {
        let mut yin_config = YinConfig::with_sample_rate(config.sample_rate);
        yin_config.f0_min_hz = config.f0_min_hz;
        yin_config.f0_max_hz = config.f0_max_hz;
        yin_config.voicing_threshold = config.voicing_threshold;
        Self {
            yin_config,
            sample_rate: config.sample_rate,
        }
    }

    /// Extract features for all words. Zero-copy slicing into the audio buffer.
    pub fn extract_all(&self, audio: &[f32], words: &[WordBoundary]) -> Vec<WordFeatures> {
        words
            .iter()
            .map(|word| {
                let start_sample =
                    ms_to_samples(word.start_ms, self.sample_rate).min(audio.len());
                let end_sample =
                    ms_to_samples(word.end_ms, self.sample_rate).min(audio.len());
                let start_sample = start_sample.min(end_sample);
                let segment = &audio[start_sample..end_sample];

                let features = self.extract_segment_inner(segment);

                WordFeatures {
                    text: word.text.clone(),
                    start_ms: word.start_ms,
                    end_ms: word.end_ms,
                    features,
                }
            })
            .collect()
    }

    /// Extract features for a single audio segment (no word boundary needed).
    /// Used by tts-service to analyze individual synthesized words.
    pub fn extract_segment(&self, audio: &[f32]) -> ProsodyFeatures {
        self.extract_segment_inner(audio)
    }

    /// Extract summary + per-frame detail for an audio segment.
    pub fn extract_frames(&self, audio: &[f32]) -> SegmentAnalysis {
        let summary = self.extract_segment_inner(audio);

        let f0_frames = estimate_f0(audio, &self.yin_config);
        let energy = rms_energy_frames(audio, self.yin_config.frame_size, self.yin_config.hop_size);
        let hop_ms = self.yin_config.hop_size as f64 / self.sample_rate as f64 * 1000.0;

        let frames = f0_frames
            .iter()
            .enumerate()
            .map(|(i, f)| FrameMeasurement {
                time_ms: i as f64 * hop_ms,
                f0_hz: f.f0_hz,
                aperiodicity: f.aperiodicity,
                energy_rms: *energy.get(i).unwrap_or(&0.0),
            })
            .collect();

        SegmentAnalysis { summary, frames }
    }

    fn extract_segment_inner(&self, segment: &[f32]) -> ProsodyFeatures {
        let energy = rms_energy(segment);

        let max_lag = (self.yin_config.sample_rate as f32 / self.yin_config.f0_min_hz) as usize;
        let min_frames_len = self.yin_config.frame_size + max_lag;

        if segment.len() < min_frames_len {
            return ProsodyFeatures {
                f0_mean_hz: None,
                f0_std_hz: None,
                energy_rms: energy,
                voicing_ratio: 0.0,
            };
        }

        let frames = estimate_f0(segment, &self.yin_config);

        if frames.is_empty() {
            return ProsodyFeatures {
                f0_mean_hz: None,
                f0_std_hz: None,
                energy_rms: energy,
                voicing_ratio: 0.0,
            };
        }

        let voiced: Vec<f32> = frames.iter().filter_map(|f| f.f0_hz).collect();
        let voicing_ratio = voiced.len() as f32 / frames.len() as f32;

        if voiced.is_empty() {
            ProsodyFeatures {
                f0_mean_hz: None,
                f0_std_hz: None,
                energy_rms: energy,
                voicing_ratio: 0.0,
            }
        } else {
            let mean = voiced.iter().sum::<f32>() / voiced.len() as f32;
            let variance =
                voiced.iter().map(|&f| (f - mean) * (f - mean)).sum::<f32>() / voiced.len() as f32;
            let std = variance.sqrt();

            ProsodyFeatures {
                f0_mean_hz: Some(mean),
                f0_std_hz: Some(std),
                energy_rms: energy,
                voicing_ratio,
            }
        }
    }
}
