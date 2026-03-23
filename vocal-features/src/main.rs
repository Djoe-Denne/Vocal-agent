use std::env;
use std::process;
use std::time::Instant;
use vocal_features::{FeatureExtractor, FeatureExtractorConfig};

const TARGET_RATE: u32 = 16_000;

fn main() {
    let path = match env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("Usage: vocal-features <path-to-wav>");
            process::exit(1);
        }
    };

    // --- I/O: read WAV into f32 mono samples ---

    let reader = hound::WavReader::open(&path).unwrap_or_else(|e| {
        eprintln!("Failed to open {path}: {e}");
        process::exit(1);
    });
    let spec = reader.spec();
    let original_rate = spec.sample_rate;

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .map(|s| s.expect("failed to read sample"))
            .collect(),
        hound::SampleFormat::Int => {
            let max = (1u32 << (spec.bits_per_sample - 1)) as f32;
            reader
                .into_samples::<i32>()
                .map(|s| s.expect("failed to read sample") as f32 / max)
                .collect()
        }
    };

    let mono: Vec<f32> = if spec.channels > 1 {
        let ch = spec.channels as usize;
        samples
            .chunks(ch)
            .map(|frame| frame.iter().sum::<f32>() / ch as f32)
            .collect()
    } else {
        samples
    };

    // --- Downsample to 16kHz if needed ---

    let (audio, sample_rate) = if original_rate != TARGET_RATE {
        let ratio = original_rate as f64 / TARGET_RATE as f64;
        let new_len = (mono.len() as f64 / ratio) as usize;
        let resampled: Vec<f32> = (0..new_len)
            .map(|i| {
                let src = i as f64 * ratio;
                let idx = src as usize;
                let frac = src - idx as f64;
                let a = mono[idx];
                let b = mono.get(idx + 1).copied().unwrap_or(a);
                a + (b - a) * frac as f32
            })
            .collect();
        eprintln!(
            "Resampled {} Hz -> {} Hz ({} -> {} samples)",
            original_rate,
            TARGET_RATE,
            mono.len(),
            resampled.len()
        );
        (resampled, TARGET_RATE)
    } else {
        (mono, original_rate)
    };

    let duration_s = audio.len() as f64 / sample_rate as f64;
    eprintln!(
        "Analyzing: {} Hz, {:.2}s, {} samples",
        sample_rate, duration_s, audio.len()
    );

    // --- Library does all the work ---

    let extractor = FeatureExtractor::new(FeatureExtractorConfig {
        sample_rate,
        ..Default::default()
    });

    let t0 = Instant::now();
    let analysis = extractor.extract_frames(&audio);
    let elapsed = t0.elapsed();

    eprintln!(
        "Extraction: {} frames in {:.3}s",
        analysis.frames.len(),
        elapsed.as_secs_f64()
    );

    println!("{}", serde_json::to_string_pretty(&analysis).unwrap());
}
