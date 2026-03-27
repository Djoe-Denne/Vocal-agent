use approx::assert_relative_eq;
use vocal_features::{
    extractor::{FeatureExtractor, FeatureExtractorConfig},
    types::WordBoundary,
};

#[test]
fn two_words_with_different_pitches() {
    let sample_rate = 16_000u32;
    let config = FeatureExtractorConfig {
        sample_rate,
        ..Default::default()
    };
    let extractor = FeatureExtractor::new(config);

    let total_samples = sample_rate as usize;
    let mut audio = vec![0.0f32; total_samples];
    let boundary_500ms = 8_000;
    let boundary_600ms = 9_600;
    for i in 0..boundary_500ms {
        audio[i] = (2.0 * std::f32::consts::PI * 200.0 * i as f32 / sample_rate as f32).sin();
    }
    for i in boundary_600ms..total_samples {
        audio[i] = (2.0 * std::f32::consts::PI * 400.0 * i as f32 / sample_rate as f32).sin();
    }

    let words = vec![
        WordBoundary {
            text: "low".to_string(),
            start_ms: 0,
            end_ms: 500,
        },
        WordBoundary {
            text: "high".to_string(),
            start_ms: 600,
            end_ms: 1000,
        },
    ];

    let features = extractor.extract_all(&audio, &words);
    assert_eq!(features.len(), 2);

    let f1 = &features[0].features;
    assert!(f1.f0_mean_hz.is_some());
    assert_relative_eq!(f1.f0_mean_hz.unwrap(), 200.0, epsilon = 10.0);
    assert!(f1.energy_rms > 0.0);
    assert!(f1.voicing_ratio > 0.8);

    let f2 = &features[1].features;
    assert!(f2.f0_mean_hz.is_some());
    assert_relative_eq!(f2.f0_mean_hz.unwrap(), 400.0, epsilon = 10.0);
    assert!(f2.energy_rms > 0.0);
    assert!(f2.voicing_ratio > 0.8);
}

#[test]
fn very_short_word_returns_partial_features() {
    let config = FeatureExtractorConfig {
        sample_rate: 16_000,
        ..Default::default()
    };
    let extractor = FeatureExtractor::new(config);
    let audio = vec![0.1f32; 16_000];

    let words = vec![WordBoundary {
        text: "x".to_string(),
        start_ms: 0,
        end_ms: 5,
    }];
    let features = extractor.extract_all(&audio, &words);

    assert_eq!(features.len(), 1);
    assert!(features[0].features.f0_mean_hz.is_none());
    assert!(features[0].features.energy_rms > 0.0);
}

#[test]
fn extract_segment_matches_extract_all_for_single_word() {
    let sample_rate = 16_000u32;
    let config = FeatureExtractorConfig {
        sample_rate,
        ..Default::default()
    };
    let extractor = FeatureExtractor::new(config);

    let audio: Vec<f32> = (0..8_000)
        .map(|i| (2.0 * std::f32::consts::PI * 300.0 * i as f32 / sample_rate as f32).sin())
        .collect();

    let via_segment = extractor.extract_segment(&audio);
    let via_all = extractor.extract_all(
        &audio,
        &[WordBoundary {
            text: "test".to_string(),
            start_ms: 0,
            end_ms: 500,
        }],
    );

    assert_eq!(via_segment.f0_mean_hz, via_all[0].features.f0_mean_hz);
    assert_eq!(via_segment.energy_rms, via_all[0].features.energy_rms);
}
