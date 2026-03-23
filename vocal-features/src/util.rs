/// Convert milliseconds to sample index.
pub fn ms_to_samples(ms: u64, sample_rate: u32) -> usize {
    (ms as u64 * sample_rate as u64 / 1000) as usize
}

/// Convert sample index to milliseconds.
pub fn samples_to_ms(samples: usize, sample_rate: u32) -> u64 {
    (samples as u64 * 1000) / sample_rate as u64
}

/// Pre-compute a Hann window of the given size.
/// Exported for consumers (phase vocoder in infra-prosody uses this).
pub fn hann_window(size: usize) -> Vec<f32> {
    if size <= 1 {
        return vec![1.0; size];
    }
    (0..size)
        .map(|i| {
            0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (size - 1) as f32).cos())
        })
        .collect()
}
