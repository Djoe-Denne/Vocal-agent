use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use audio_domain::{AudioTransformPort, AudioTransformRequest};

use crate::{ApplicationError, TransformAudioRequest, TransformAudioResponse};

#[async_trait]
pub trait TransformAudioUseCase: Send + Sync {
    async fn transform_audio(
        &self,
        request: TransformAudioRequest,
    ) -> Result<TransformAudioResponse, ApplicationError>;
}

pub struct TransformAudioUseCaseImpl {
    transformer: Arc<dyn AudioTransformPort>,
    default_sample_rate_hz: u32,
}

impl TransformAudioUseCaseImpl {
    pub fn new(transformer: Arc<dyn AudioTransformPort>, default_sample_rate_hz: u32) -> Self {
        Self {
            transformer,
            default_sample_rate_hz,
        }
    }
}

#[async_trait]
impl TransformAudioUseCase for TransformAudioUseCaseImpl {
    async fn transform_audio(
        &self,
        request: TransformAudioRequest,
    ) -> Result<TransformAudioResponse, ApplicationError> {
        let source_sample_rate_hz = request.sample_rate_hz.unwrap_or(self.default_sample_rate_hz);
        let target_sample_rate_hz = request
            .target_sample_rate_hz
            .unwrap_or(self.default_sample_rate_hz);
        let session_id = request
            .session_id
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        tracing::debug!(
            session_id = %session_id,
            input_samples = request.samples.len(),
            source_sample_rate_hz,
            target_sample_rate_hz,
            "starting audio transformation"
        );

        let transformed = self
            .transformer
            .transform(AudioTransformRequest {
                samples: request.samples,
                source_sample_rate_hz,
                target_sample_rate_hz,
            })
            .await?;

        tracing::debug!(
            session_id = %session_id,
            output_samples = transformed.samples.len(),
            output_sample_rate_hz = transformed.sample_rate_hz,
            resampled = transformed.metadata.resampled,
            "audio transformation completed"
        );

        Ok(TransformAudioResponse {
            session_id,
            samples: transformed.samples,
            sample_rate_hz: transformed.sample_rate_hz,
            metadata: transformed.metadata,
        })
    }
}
