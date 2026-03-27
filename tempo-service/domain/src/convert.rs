pub fn ms_to_samples(ms: u64, sample_rate_hz: u32) -> usize {
    ((ms as u64 * sample_rate_hz as u64) / 1000) as usize
}

pub fn samples_to_ms(samples: usize, sample_rate_hz: u32) -> u64 {
    if sample_rate_hz == 0 {
        return 0;
    }
    (samples as u64 * 1000) / sample_rate_hz as u64
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
}
