pub fn ms_to_samples(ms: u64, sample_rate_hz: u32) -> usize {
    ((ms as u64 * sample_rate_hz as u64) / 1000) as usize
}

pub fn samples_to_ms(samples: usize, sample_rate_hz: u32) -> u64 {
    if sample_rate_hz == 0 {
        return 0;
    }
    (samples as u64 * 1000) / sample_rate_hz as u64
}

/// Convert an index in the analysis buffer to the useful (margin-free) coordinate.
/// Returns `None` if the index falls within the left margin.
pub fn analysis_to_useful(sample_idx: usize, useful_start: usize) -> Option<usize> {
    sample_idx.checked_sub(useful_start)
}

/// Convert an index in useful (margin-free) coordinates back to analysis buffer coordinates.
pub fn useful_to_analysis(sample_idx: usize, useful_start: usize) -> usize {
    sample_idx + useful_start
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ms_to_samples_basic() {
        assert_eq!(ms_to_samples(1000, 16_000), 16_000);
        assert_eq!(ms_to_samples(500, 16_000), 8_000);
        assert_eq!(ms_to_samples(0, 16_000), 0);
    }

    #[test]
    fn samples_to_ms_basic() {
        assert_eq!(samples_to_ms(16_000, 16_000), 1000);
        assert_eq!(samples_to_ms(8_000, 16_000), 500);
        assert_eq!(samples_to_ms(0, 16_000), 0);
    }

    #[test]
    fn samples_to_ms_zero_rate_returns_zero() {
        assert_eq!(samples_to_ms(1000, 0), 0);
    }

    #[test]
    fn round_trip_consistency() {
        let ms = 750u64;
        let rate = 22_050u32;
        let samples = ms_to_samples(ms, rate);
        let back = samples_to_ms(samples, rate);
        assert!((back as i64 - ms as i64).unsigned_abs() <= 1);
    }

    #[test]
    fn analysis_to_useful_within_margin_returns_none() {
        assert_eq!(analysis_to_useful(5, 10), None);
    }

    #[test]
    fn analysis_to_useful_at_boundary() {
        assert_eq!(analysis_to_useful(10, 10), Some(0));
    }

    #[test]
    fn analysis_useful_round_trip() {
        let useful_start = 160;
        let useful_idx = 42;
        let analysis_idx = useful_to_analysis(useful_idx, useful_start);
        assert_eq!(analysis_idx, 202);
        assert_eq!(analysis_to_useful(analysis_idx, useful_start), Some(useful_idx));
    }
}
