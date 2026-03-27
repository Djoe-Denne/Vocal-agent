/// RMS energy of an audio segment.
pub fn rms_energy(audio: &[f32]) -> f32 {
    if audio.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = audio.iter().map(|&s| s * s).sum();
    (sum_sq / audio.len() as f32).sqrt()
}

/// RMS energy per frame, same framing as YIN.
pub fn rms_energy_frames(audio: &[f32], frame_size: usize, hop_size: usize) -> Vec<f32> {
    if frame_size == 0 || hop_size == 0 {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut start = 0;
    while start + frame_size <= audio.len() {
        result.push(rms_energy(&audio[start..start + frame_size]));
        start += hop_size;
    }
    result
}
