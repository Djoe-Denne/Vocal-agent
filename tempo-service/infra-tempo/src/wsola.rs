use crate::WsolaConfig;

/// WSOLA (Waveform Similarity Overlap-Add) time-stretcher for speech.
///
/// Stretches or compresses an audio segment by `ratio` (> 1 = slower, < 1 = faster)
/// without changing pitch.
pub fn wsola_stretch(samples: &[f32], ratio: f64, config: &WsolaConfig) -> Vec<f32> {
    if samples.is_empty() || (ratio - 1.0).abs() < f64::from(config.stretch_tolerance) {
        return samples.to_vec();
    }

    let ratio = ratio.clamp(
        f64::from(config.min_stretch_ratio),
        f64::from(config.max_stretch_ratio),
    );

    let window_size = ms_to_samples(config.window_ms, config.sample_rate_hz);
    // Synthesis hop is FIXED — determines proper overlap-add in output.
    let hop_s = ((window_size as f64) * (1.0 - f64::from(config.overlap_ratio))) as usize;
    if hop_s == 0 || window_size == 0 || window_size > samples.len() {
        return stretch_linear(samples, ratio);
    }

    // Analysis hop VARIES with ratio — determines how densely we read input.
    let hop_a = (hop_s as f64 / ratio).round().max(1.0) as usize;

    let target_len = (samples.len() as f64 * ratio).round() as usize;
    let alloc_len = target_len + window_size;
    let mut output = vec![0.0f32; alloc_len];
    let mut norm = vec![0.0f32; alloc_len];
    let search_range = (hop_a / 2).max(ms_to_samples(2, config.sample_rate_hz));
    let window = hann_window(window_size);

    let mut write_pos: usize = 0;
    let mut read_pos: f64 = 0.0;

    loop {
        let read_idx = read_pos.round() as usize;
        if read_idx + window_size > samples.len() || write_pos + window_size > alloc_len {
            break;
        }

        let best_offset = if write_pos == 0 {
            0i32
        } else {
            find_best_overlap(
                &output,
                write_pos,
                samples,
                read_idx,
                window_size,
                search_range,
            )
        };

        let src_start = (read_idx as i64 + best_offset as i64)
            .max(0)
            .min((samples.len() - window_size) as i64) as usize;

        for i in 0..window_size {
            output[write_pos + i] += samples[src_start + i] * window[i];
            norm[write_pos + i] += window[i];
        }

        write_pos += hop_s;
        read_pos += hop_a as f64;
    }

    for i in 0..alloc_len {
        if norm[i] > 1e-8 {
            output[i] /= norm[i];
        }
    }

    output.truncate(target_len.min(output.len()));
    output
}

/// Find the offset within `[-search_range, search_range]` that maximises cross-correlation
/// between the tail of `output` and the candidate window from `source`.
fn find_best_overlap(
    output: &[f32],
    write_pos: usize,
    source: &[f32],
    read_pos: usize,
    window_size: usize,
    search_range: usize,
) -> i32 {
    let overlap_len = window_size / 4;
    if write_pos < overlap_len {
        return 0;
    }

    let ref_start = write_pos - overlap_len;
    let ref_slice = &output[ref_start..write_pos];

    let mut best_offset: i32 = 0;
    let mut best_corr = f64::NEG_INFINITY;

    let min_off = -(search_range as i32);
    let max_off = search_range as i32;

    for offset in min_off..=max_off {
        let candidate_start = read_pos as i64 + offset as i64 - overlap_len as i64;
        if candidate_start < 0 || (candidate_start as usize + overlap_len) > source.len() {
            continue;
        }
        let candidate = &source[candidate_start as usize..candidate_start as usize + overlap_len];

        let corr = normalized_cross_correlation(ref_slice, candidate);
        if corr > best_corr {
            best_corr = corr;
            best_offset = offset;
        }
    }

    best_offset
}

fn normalized_cross_correlation(a: &[f32], b: &[f32]) -> f64 {
    let mut dot = 0.0f64;
    let mut energy_a = 0.0f64;
    let mut energy_b = 0.0f64;

    for (&x, &y) in a.iter().zip(b.iter()) {
        let x = x as f64;
        let y = y as f64;
        dot += x * y;
        energy_a += x * x;
        energy_b += y * y;
    }

    let denom = (energy_a * energy_b).sqrt();
    if denom < 1e-12 {
        return 0.0;
    }
    dot / denom
}

fn hann_window(size: usize) -> Vec<f32> {
    (0..size)
        .map(|i| {
            let phase = std::f32::consts::PI * 2.0 * i as f32 / size as f32;
            0.5 * (1.0 - phase.cos())
        })
        .collect()
}

/// Raised-cosine crossfade between `a` (ending) and `b` (starting).
pub fn crossfade(a: &[f32], b: &[f32], crossfade_samples: usize) -> Vec<f32> {
    let fade_len = crossfade_samples.min(a.len()).min(b.len());
    if fade_len == 0 {
        let mut out = a.to_vec();
        out.extend_from_slice(b);
        return out;
    }

    let a_body = &a[..a.len() - fade_len];
    let a_tail = &a[a.len() - fade_len..];
    let b_head = &b[..fade_len];
    let b_body = &b[fade_len..];

    let mut out = Vec::with_capacity(a_body.len() + fade_len + b_body.len());
    out.extend_from_slice(a_body);

    for i in 0..fade_len {
        let t = i as f32 / fade_len as f32;
        let w = 0.5 * (1.0 - (std::f32::consts::PI * t).cos());
        out.push(a_tail[i] * (1.0 - w) + b_head[i] * w);
    }

    out.extend_from_slice(b_body);
    out
}

/// Fallback linear interpolation stretcher for very short segments.
fn stretch_linear(samples: &[f32], ratio: f64) -> Vec<f32> {
    if samples.is_empty() {
        return Vec::new();
    }
    let target_len = (samples.len() as f64 * ratio).round().max(1.0) as usize;
    if target_len <= 1 {
        return vec![samples[0]];
    }

    let mut output = Vec::with_capacity(target_len);
    let max_idx = samples.len() - 1;
    for i in 0..target_len {
        let src_pos = i as f64 * max_idx as f64 / (target_len - 1) as f64;
        let left = src_pos.floor() as usize;
        let right = (left + 1).min(max_idx);
        let frac = (src_pos - left as f64) as f32;
        output.push(samples[left] * (1.0 - frac) + samples[right] * frac);
    }
    output
}

fn ms_to_samples(ms: u32, sample_rate_hz: u32) -> usize {
    (sample_rate_hz as usize * ms as usize) / 1000
}

/// Compute RMS energy in decibels for a slice of samples.
pub fn rms_db(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return f32::NEG_INFINITY;
    }
    let mean_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>()
        / samples.len() as f64;
    if mean_sq < 1e-20 {
        return f32::NEG_INFINITY;
    }
    (10.0 * mean_sq.log10()) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine_wave(freq_hz: f32, sample_rate: u32, duration_ms: u32) -> Vec<f32> {
        let num_samples = (sample_rate as usize * duration_ms as usize) / 1000;
        (0..num_samples)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                (2.0 * std::f32::consts::PI * freq_hz * t).sin()
            })
            .collect()
    }

    #[test]
    fn wsola_stretch_preserves_near_unity() {
        let config = WsolaConfig::default();
        let input = sine_wave(440.0, 16000, 200);
        let output = wsola_stretch(&input, 1.02, &config);
        assert_eq!(output.len(), input.len());
    }

    #[test]
    fn wsola_stretch_doubles_length() {
        let config = WsolaConfig::default();
        let input = sine_wave(440.0, 16000, 200);
        let output = wsola_stretch(&input, 2.0, &config);
        let expected = input.len() * 2;
        let tolerance = (expected as f64 * 0.1) as usize;
        assert!(
            (output.len() as i64 - expected as i64).unsigned_abs() as usize <= tolerance,
            "expected ~{expected} samples, got {}",
            output.len()
        );
    }

    #[test]
    fn wsola_stretch_halves_length() {
        let config = WsolaConfig::default();
        let input = sine_wave(440.0, 16000, 200);
        let output = wsola_stretch(&input, 0.5, &config);
        let expected = input.len() / 2;
        let tolerance = (expected as f64 * 0.15) as usize;
        assert!(
            (output.len() as i64 - expected as i64).unsigned_abs() as usize <= tolerance,
            "expected ~{expected} samples, got {}",
            output.len()
        );
    }

    #[test]
    fn crossfade_blends_segments() {
        let a = vec![1.0f32; 100];
        let b = vec![0.0f32; 100];
        let result = crossfade(&a, &b, 20);
        assert_eq!(result.len(), 180);
        assert!((result[80] - 1.0).abs() < 0.01);
        assert!(result[90] > 0.2 && result[90] < 0.8);
    }

    #[test]
    fn rms_db_of_silence_is_negative_infinity() {
        let silence = vec![0.0f32; 100];
        assert_eq!(rms_db(&silence), f32::NEG_INFINITY);
    }

    #[test]
    fn rms_db_of_unit_signal() {
        let signal = vec![1.0f32; 100];
        let db = rms_db(&signal);
        assert!((db - 0.0).abs() < 0.1);
    }
}
