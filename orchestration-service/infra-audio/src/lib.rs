use std::time::Duration;

use async_trait::async_trait;
use audio_grpc_server::{pb, AudioServiceClient};
use orchestration_domain::{DomainError, PipelineContext, PipelineStage};
use serde_json::json;
use tonic::transport::{Channel, Endpoint};
use tonic::Request;

pub struct AudioTransformStage {
    client: AudioServiceClient<Channel>,
    request_timeout: Duration,
    target_sample_rate_hz: Option<u32>,
}

impl AudioTransformStage {
    pub fn new(
        client: AudioServiceClient<Channel>,
        request_timeout: Duration,
        target_sample_rate_hz: Option<u32>,
    ) -> Self {
        Self {
            client,
            request_timeout,
            target_sample_rate_hz,
        }
    }
}

#[async_trait]
impl PipelineStage for AudioTransformStage {
    fn name(&self) -> &'static str {
        "audio_transform"
    }

    async fn execute(&self, context: &mut PipelineContext) -> Result<(), DomainError> {
        let mut client = self.client.clone();
        let request = pb::TransformAudioRequest {
            samples: context.audio.samples.clone(),
            sample_rate_hz: Some(context.audio.sample_rate_hz),
            target_sample_rate_hz: self.target_sample_rate_hz,
            session_id: Some(context.session_id.clone()),
        };
        let rpc = client.transform_audio(Request::new(request));
        let response = tokio::time::timeout(self.request_timeout, rpc)
            .await
            .map_err(|_| DomainError::external_service_error("audio", "gRPC request timed out"))?
            .map_err(|status| map_status("audio", status))?
            .into_inner();

        context.session_id = response.session_id;
        context.audio.samples = response.samples;
        context.audio.sample_rate_hz = response.sample_rate_hz;
        if let Some(metadata) = response.metadata {
            context.set_extension(
                "audio.transform",
                json!({
                    "clamped": metadata.clamped,
                    "resampled": metadata.resampled,
                    "input_sample_count": metadata.input_sample_count,
                    "output_sample_count": metadata.output_sample_count,
                    "source_sample_rate_hz": metadata.source_sample_rate_hz,
                    "target_sample_rate_hz": metadata.target_sample_rate_hz,
                }),
            );
        }
        Ok(())
    }
}

pub async fn connect_audio_client(
    endpoint_uri: &str,
    connect_timeout: Duration,
    max_decoding_message_bytes: usize,
    max_encoding_message_bytes: usize,
) -> Result<AudioServiceClient<Channel>, DomainError> {
    let endpoint = Endpoint::from_shared(endpoint_uri.to_string())
        .map_err(|err| DomainError::internal_error(&format!("invalid audio endpoint: {err}")))?
        .connect_timeout(connect_timeout);
    let channel = endpoint.connect().await.map_err(|err| {
        DomainError::external_service_error("audio", &format!("failed to connect: {err}"))
    })?;
    Ok(AudioServiceClient::new(channel)
        .max_decoding_message_size(max_decoding_message_bytes)
        .max_encoding_message_size(max_encoding_message_bytes))
}

fn map_status(service: &str, status: tonic::Status) -> DomainError {
    DomainError::external_service_error(
        service,
        &format!("gRPC {}: {}", status.code(), status.message()),
    )
}
