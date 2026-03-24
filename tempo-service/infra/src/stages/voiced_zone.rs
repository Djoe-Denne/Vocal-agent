use tempo_domain::{
    DomainError, SegmentVoicedRegions, TempoPipelineContext, TempoPipelineStage, VoicedRegion,
};

const MIN_VOICED_ZONE_MS: u64 = 30;

/// Step 6: group consecutive voiced pitch frames into continuous voiced regions.
///
/// Filters out regions shorter than ~30ms and computes a stability score
/// based on F0 variance within each region.
pub struct VoicedZoneStage;

impl TempoPipelineStage for VoicedZoneStage {
    fn name(&self) -> &'static str {
        "voiced_zone"
    }

    fn execute(&self, context: &mut TempoPipelineContext) -> Result<(), DomainError> {
        if context.pitch_data.is_empty() {
            return Err(DomainError::internal_error(
                "voiced_zone: no pitch data available",
            ));
        }

        let rate = context.sample_rate_hz;
        let min_zone_samples = tempo_domain::convert::ms_to_samples(MIN_VOICED_ZONE_MS, rate);

        let mut all_regions = Vec::with_capacity(context.pitch_data.len());

        for pitch_data in &context.pitch_data {
            let frames = &pitch_data.frames;
            let mut regions: Vec<VoicedRegion> = Vec::new();

            let mut run_start: Option<usize> = None;

            for (i, frame) in frames.iter().enumerate() {
                if frame.voiced {
                    if run_start.is_none() {
                        run_start = Some(i);
                    }
                } else if let Some(start_idx) = run_start.take() {
                    if let Some(region) = build_region(frames, start_idx, i, min_zone_samples) {
                        regions.push(region);
                    }
                }
            }
            if let Some(start_idx) = run_start.take() {
                if let Some(region) =
                    build_region(frames, start_idx, frames.len(), min_zone_samples)
                {
                    regions.push(region);
                }
            }

            tracing::trace!(
                segment_index = pitch_data.segment_index,
                region_count = regions.len(),
                "voiced zone construction complete for segment"
            );

            all_regions.push(SegmentVoicedRegions {
                segment_index: pitch_data.segment_index,
                regions,
            });
        }

        tracing::debug!(
            segment_count = all_regions.len(),
            total_regions = all_regions.iter().map(|r| r.regions.len()).sum::<usize>(),
            "voiced zone construction complete"
        );

        context.voiced_regions = all_regions;
        Ok(())
    }
}

fn build_region(
    frames: &[tempo_domain::PitchFrame],
    start_idx: usize,
    end_idx: usize,
    min_zone_samples: usize,
) -> Option<VoicedRegion> {
    let run = &frames[start_idx..end_idx];
    if run.is_empty() {
        return None;
    }

    let start_sample = run.first().unwrap().center_sample;
    let end_sample = run.last().unwrap().center_sample;

    if end_sample <= start_sample {
        return None;
    }

    let duration = end_sample - start_sample;
    if duration < min_zone_samples {
        return None;
    }

    let voiced_frames: Vec<f32> = run.iter().filter(|f| f.voiced).map(|f| f.f0_hz).collect();
    if voiced_frames.is_empty() {
        return None;
    }

    let mean_f0 = voiced_frames.iter().sum::<f32>() / voiced_frames.len() as f32;

    let mean_period: f32 = run
        .iter()
        .filter(|f| f.voiced)
        .map(|f| f.period_samples)
        .sum::<f32>()
        / voiced_frames.len() as f32;

    let stability_score = if mean_f0 > 0.0 && voiced_frames.len() > 1 {
        let variance = voiced_frames
            .iter()
            .map(|f| (f - mean_f0) * (f - mean_f0))
            .sum::<f32>()
            / voiced_frames.len() as f32;
        let std_dev = variance.sqrt();
        (1.0 - std_dev / mean_f0).clamp(0.0, 1.0)
    } else {
        1.0
    };

    Some(VoicedRegion {
        start_sample,
        end_sample,
        mean_f0,
        mean_period_samples: mean_period,
        stability_score,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempo_domain::{PitchFrame, SegmentPitchData, TempoPipelineContext};

    fn pitch_frame(center: usize, voiced: bool, f0: f32, period: f32) -> PitchFrame {
        PitchFrame {
            center_sample: center,
            voiced,
            f0_hz: f0,
            period_samples: period,
        }
    }

    fn ctx_with_pitch(frames: Vec<PitchFrame>) -> TempoPipelineContext {
        let mut ctx = TempoPipelineContext::new(vec![0.0; 16000], 16_000, Vec::new(), Vec::new());
        ctx.pitch_data = vec![SegmentPitchData {
            segment_index: 0,
            frames,
        }];
        ctx
    }

    #[test]
    fn groups_consecutive_voiced_frames() {
        let frames = vec![
            pitch_frame(80, true, 200.0, 80.0),
            pitch_frame(240, true, 205.0, 78.0),
            pitch_frame(400, true, 198.0, 81.0),
            pitch_frame(560, true, 202.0, 79.0),
            pitch_frame(720, true, 200.0, 80.0),
            pitch_frame(880, false, 0.0, 0.0),
            pitch_frame(1040, true, 150.0, 107.0),
            pitch_frame(1200, true, 148.0, 108.0),
            pitch_frame(1360, true, 152.0, 105.0),
            pitch_frame(1520, true, 150.0, 107.0),
        ];
        let mut ctx = ctx_with_pitch(frames);

        let stage = VoicedZoneStage;
        stage.execute(&mut ctx).expect("should succeed");

        assert_eq!(ctx.voiced_regions.len(), 1);
        assert_eq!(ctx.voiced_regions[0].regions.len(), 2);

        let r0 = &ctx.voiced_regions[0].regions[0];
        assert_eq!(r0.start_sample, 80);
        assert_eq!(r0.end_sample, 720);
        assert!(r0.stability_score > 0.9);

        let r1 = &ctx.voiced_regions[0].regions[1];
        assert_eq!(r1.start_sample, 1040);
        assert_eq!(r1.end_sample, 1520);
    }

    #[test]
    fn filters_out_short_regions() {
        let frames = vec![
            pitch_frame(80, true, 200.0, 80.0),
            pitch_frame(120, true, 200.0, 80.0), // only 40 samples span
            pitch_frame(200, false, 0.0, 0.0),
        ];
        let mut ctx = ctx_with_pitch(frames);

        let stage = VoicedZoneStage;
        stage.execute(&mut ctx).expect("should succeed");

        // 40 samples < 480 samples (30ms at 16kHz), so filtered out
        assert!(ctx.voiced_regions[0].regions.is_empty());
    }

    #[test]
    fn stability_score_is_low_for_variable_f0() {
        let frames = vec![
            pitch_frame(0, true, 100.0, 160.0),
            pitch_frame(160, true, 300.0, 53.0),
            pitch_frame(320, true, 100.0, 160.0),
            pitch_frame(480, true, 300.0, 53.0),
            pitch_frame(640, true, 100.0, 160.0),
            pitch_frame(800, true, 300.0, 53.0),
        ];
        let mut ctx = ctx_with_pitch(frames);

        let stage = VoicedZoneStage;
        stage.execute(&mut ctx).expect("should succeed");

        let r = &ctx.voiced_regions[0].regions[0];
        assert!(
            r.stability_score < 0.6,
            "stability {:.2} should be low for variable F0",
            r.stability_score
        );
    }

    #[test]
    fn rejects_empty_pitch_data() {
        let mut ctx = TempoPipelineContext::new(vec![0.0; 100], 16_000, Vec::new(), Vec::new());
        let stage = VoicedZoneStage;
        assert!(stage.execute(&mut ctx).is_err());
    }
}
