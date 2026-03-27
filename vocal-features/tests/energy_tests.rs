use approx::assert_relative_eq;
use vocal_features::energy::rms_energy;

#[test]
fn silence_has_zero_energy() {
    assert_eq!(rms_energy(&[0.0; 1000]), 0.0);
}

#[test]
fn empty_has_zero_energy() {
    assert_eq!(rms_energy(&[]), 0.0);
}

#[test]
fn constant_signal_rms_equals_value() {
    assert_relative_eq!(rms_energy(&[0.5; 1000]), 0.5, epsilon = 1e-6);
}

#[test]
fn sine_rms_is_one_over_sqrt2() {
    let audio: Vec<f32> = (0..16_000)
        .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 16_000.0).sin())
        .collect();
    assert_relative_eq!(rms_energy(&audio), 1.0 / 2.0f32.sqrt(), epsilon = 0.01);
}
