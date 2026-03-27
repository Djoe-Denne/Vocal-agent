use approx::assert_relative_eq;
use vocal_features::yin::{estimate_f0, estimate_mean_f0, YinConfig};

#[test]
fn sine_440hz_returns_correct_f0() {
    let sample_rate = 16_000u32;
    let config = YinConfig::with_sample_rate(sample_rate);
    let audio: Vec<f32> = (0..sample_rate as usize)
        .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sample_rate as f32).sin())
        .collect();

    let frames = estimate_f0(&audio, &config);
    let voiced: Vec<f32> = frames.iter().filter_map(|f| f.f0_hz).collect();

    assert!(!voiced.is_empty(), "should detect voiced frames");
    for &f0 in &voiced {
        assert_relative_eq!(f0, 440.0, epsilon = 3.0);
    }
}

#[test]
fn sine_100hz_returns_correct_f0() {
    let sample_rate = 16_000u32;
    let config = YinConfig::with_sample_rate(sample_rate);
    let audio: Vec<f32> = (0..sample_rate as usize)
        .map(|i| (2.0 * std::f32::consts::PI * 100.0 * i as f32 / sample_rate as f32).sin())
        .collect();

    let mean = estimate_mean_f0(&audio, &config);
    assert!(mean.is_some());
    assert_relative_eq!(mean.unwrap(), 100.0, epsilon = 3.0);
}

#[test]
fn silence_returns_unvoiced() {
    let config = YinConfig::with_sample_rate(16_000);
    let audio = vec![0.0f32; 16_000];
    let frames = estimate_f0(&audio, &config);
    assert!(frames.iter().all(|f| f.f0_hz.is_none()));
}

#[test]
fn white_noise_mostly_unvoiced() {
    let config = YinConfig::with_sample_rate(16_000);
    // LCG pseudo-random noise — aperiodic over 16k samples, unlike sin-based generators.
    let mut state = 12345u32;
    let audio: Vec<f32> = (0..16_000)
        .map(|_| {
            state = state.wrapping_mul(1103515245).wrapping_add(12345);
            (state as f32 / u32::MAX as f32) * 2.0 - 1.0
        })
        .collect();
    let frames = estimate_f0(&audio, &config);
    let voiced_ratio =
        frames.iter().filter(|f| f.f0_hz.is_some()).count() as f32 / frames.len() as f32;
    assert!(
        voiced_ratio < 0.3,
        "noise should be mostly unvoiced, got {voiced_ratio}"
    );
}

#[test]
fn two_tones_detected() {
    let sample_rate = 16_000u32;
    let config = YinConfig::with_sample_rate(sample_rate);
    let half = sample_rate as usize / 2;
    let mut audio = Vec::with_capacity(sample_rate as usize);
    for i in 0..half {
        audio.push((2.0 * std::f32::consts::PI * 200.0 * i as f32 / sample_rate as f32).sin());
    }
    for i in 0..half {
        audio.push((2.0 * std::f32::consts::PI * 400.0 * i as f32 / sample_rate as f32).sin());
    }

    let frames = estimate_f0(&audio, &config);
    let mid_frame = frames.len() / 2;
    let first_half: Vec<f32> = frames[..mid_frame].iter().filter_map(|f| f.f0_hz).collect();
    let second_half: Vec<f32> = frames[mid_frame..].iter().filter_map(|f| f.f0_hz).collect();

    let mean_first: f32 = first_half.iter().sum::<f32>() / first_half.len() as f32;
    let mean_second: f32 = second_half.iter().sum::<f32>() / second_half.len() as f32;

    assert_relative_eq!(mean_first, 200.0, epsilon = 10.0);
    assert_relative_eq!(mean_second, 400.0, epsilon = 10.0);
}

#[test]
fn estimate_mean_f0_on_silence_returns_none() {
    let config = YinConfig::with_sample_rate(16_000);
    assert!(estimate_mean_f0(&vec![0.0; 16_000], &config).is_none());
}
