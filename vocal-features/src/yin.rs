/// Configuration for the YIN estimator.
/// All frame/hop sizes are in samples; use `with_sample_rate` for ms-based defaults.
#[derive(Debug, Clone)]
pub struct YinConfig {
    pub sample_rate: u32,
    pub frame_size: usize,
    pub hop_size: usize,
    pub f0_min_hz: f32,
    pub f0_max_hz: f32,
    pub voicing_threshold: f32,
}

impl YinConfig {
    /// Create config with standard 25ms frame / 10ms hop for the given sample rate.
    pub fn with_sample_rate(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            frame_size: (sample_rate as f32 * 0.025) as usize,
            hop_size: (sample_rate as f32 * 0.010) as usize,
            f0_min_hz: 50.0,
            f0_max_hz: 500.0,
            voicing_threshold: 0.3,
        }
    }
}

impl Default for YinConfig {
    fn default() -> Self {
        Self::with_sample_rate(16_000)
    }
}

/// Per-frame F0 estimate.
#[derive(Debug, Clone, Copy)]
pub struct F0Frame {
    /// F0 in Hz, or None if unvoiced.
    pub f0_hz: Option<f32>,
    /// YIN aperiodicity (0.0 = periodic, 1.0 = noise).
    pub aperiodicity: f32,
}

/// Estimate F0 for each frame in the audio signal.
/// Returns one `F0Frame` per hop.
pub fn estimate_f0(audio: &[f32], config: &YinConfig) -> Vec<F0Frame> {
    let max_lag = (config.sample_rate as f32 / config.f0_min_hz) as usize;
    let min_lag = (config.sample_rate as f32 / config.f0_max_hz) as usize;

    let required_len = config.frame_size + max_lag;
    if audio.len() < required_len {
        return Vec::new();
    }

    let num_frames = (audio.len() - required_len) / config.hop_size + 1;
    let mut results = Vec::with_capacity(num_frames);

    let mut d = vec![0.0f32; max_lag + 1];
    let mut d_prime = vec![0.0f32; max_lag + 1];

    let mut t = 0;
    while t + config.frame_size + max_lag <= audio.len() {
        // Step 1: Difference function.
        // Pre-slicing + iterator zip helps LLVM eliminate bounds checks and auto-vectorize.
        d[0] = 0.0;
        let base = &audio[t..t + config.frame_size];
        for tau in 1..=max_lag {
            let shifted = &audio[t + tau..t + tau + config.frame_size];
            d[tau] = base
                .iter()
                .zip(shifted)
                .map(|(&a, &b)| {
                    let diff = a - b;
                    diff * diff
                })
                .sum();
        }

        // Step 2: Cumulative mean normalized difference function
        d_prime[0] = 1.0;
        let mut running_sum = 0.0f32;
        for tau in 1..=max_lag {
            running_sum += d[tau];
            if running_sum < 1e-10 {
                d_prime[tau] = 1.0;
            } else {
                d_prime[tau] = d[tau] / (running_sum / tau as f32);
            }
        }

        // Step 3: Absolute threshold — find the first local minimum of d' below threshold.
        // Per the YIN paper (step 4): "the smallest value of tau that gives a minimum of d'
        // deeper than that threshold." A local minimum is where d'[tau] <= d'[tau+1].
        let mut chosen_tau: Option<usize> = None;
        for tau in min_lag..max_lag {
            if d_prime[tau] < config.voicing_threshold && d_prime[tau] <= d_prime[tau + 1] {
                chosen_tau = Some(tau);
                break;
            }
        }
        if chosen_tau.is_none() && d_prime[max_lag] < config.voicing_threshold {
            chosen_tau = Some(max_lag);
        }

        let frame = match chosen_tau {
            None => F0Frame {
                f0_hz: None,
                aperiodicity: 1.0,
            },
            Some(tau) => {
                // Step 4: Parabolic interpolation
                let refined_tau = if tau > 0 && tau < max_lag {
                    let a = d_prime[tau - 1];
                    let b = d_prime[tau];
                    let c = d_prime[tau + 1];
                    let denom = 2.0 * (a - 2.0 * b + c);
                    if denom.abs() > 1e-10 {
                        let offset = (a - c) / denom;
                        let refined = tau as f32 + offset;
                        if refined >= (tau - 1) as f32 && refined <= (tau + 1) as f32 {
                            refined
                        } else {
                            tau as f32
                        }
                    } else {
                        tau as f32
                    }
                } else {
                    tau as f32
                };

                // Step 5: Convert lag to frequency
                let f0 = config.sample_rate as f32 / refined_tau;
                F0Frame {
                    f0_hz: Some(f0),
                    aperiodicity: d_prime[tau],
                }
            }
        };

        results.push(frame);
        t += config.hop_size;
    }

    results
}

/// Convenience: single mean F0 for an entire segment.
/// Used by tts-service to get source F0 of a synthesized word before pitch-shifting.
/// Returns None if no voiced frames found.
pub fn estimate_mean_f0(audio: &[f32], config: &YinConfig) -> Option<f32> {
    let frames = estimate_f0(audio, config);
    let voiced: Vec<f32> = frames.iter().filter_map(|f| f.f0_hz).collect();
    if voiced.is_empty() {
        None
    } else {
        Some(voiced.iter().sum::<f32>() / voiced.len() as f32)
    }
}
