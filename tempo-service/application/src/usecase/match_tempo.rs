use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use tempo_domain::{TempoMatchPort, TempoMatchRequest};

use crate::{ApplicationError, MatchTempoRequest, MatchTempoResponse};

#[async_trait]
pub trait TempoMatchUseCase: Send + Sync {
    async fn match_tempo(
        &self,
        request: MatchTempoRequest,
    ) -> Result<MatchTempoResponse, ApplicationError>;
}

pub struct TempoMatchUseCaseImpl {
    matcher: Arc<dyn TempoMatchPort>,
    default_sample_rate_hz: u32,
}

impl TempoMatchUseCaseImpl {
    pub fn new(matcher: Arc<dyn TempoMatchPort>, default_sample_rate_hz: u32) -> Self {
        Self {
            matcher,
            default_sample_rate_hz,
        }
    }
}

#[async_trait]
impl TempoMatchUseCase for TempoMatchUseCaseImpl {
    async fn match_tempo(
        &self,
        request: MatchTempoRequest,
    ) -> Result<MatchTempoResponse, ApplicationError> {
        let sample_rate_hz = request.tts_sample_rate_hz.unwrap_or(self.default_sample_rate_hz);
        let session_id = request
            .session_id
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        let output = self
            .matcher
            .match_tempo(TempoMatchRequest {
                tts_samples: request.tts_samples,
                tts_sample_rate_hz: sample_rate_hz,
                original_timings: request.original_timings,
                tts_timings: request.tts_timings,
            })
            .await?;

        Ok(MatchTempoResponse {
            session_id,
            samples: output.samples,
            sample_rate_hz: output.sample_rate_hz,
        })
    }
}
