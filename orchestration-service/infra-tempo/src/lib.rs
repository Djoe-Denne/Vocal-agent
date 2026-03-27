use std::time::Duration;

use async_trait::async_trait;
use orchestration_domain::{DomainError, PipelineContext, PipelineStage};
use tempo_grpc_server::{pb, TempoServiceClient};
use tonic::transport::{Channel, Endpoint};
use tonic::Request;

pub struct TempoMatchStage {
    client: TempoServiceClient<Channel>,
    request_timeout: Duration,
}

impl TempoMatchStage {
    pub fn new(client: TempoServiceClient<Channel>, request_timeout: Duration) -> Self {
        Self {
            client,
            request_timeout,
        }
    }
}

#[async_trait]
impl PipelineStage for TempoMatchStage {
    fn name(&self) -> &'static str {
        "tempo_match"
    }

    async fn execute(&self, context: &mut PipelineContext) -> Result<(), DomainError> {
        if context.audio.samples.is_empty() {
            return Err(DomainError::internal_error(
                "tempo_match: audio samples are empty",
            ));
        }

        let tts_timings = map_orch_to_proto_timings(&context.aligned_words);
        if tts_timings.is_empty() {
            return Err(DomainError::internal_error(
                "tempo_match: no TTS aligned words available",
            ));
        }

        let original_timings = extract_original_timings(context)?;
        if original_timings.is_empty() {
            return Err(DomainError::internal_error(
                "tempo_match: no original timings available in extensions",
            ));
        }

        tracing::debug!(
            tts_sample_count = context.audio.samples.len(),
            tts_sample_rate_hz = context.audio.sample_rate_hz,
            tts_timing_count = tts_timings.len(),
            original_timing_count = original_timings.len(),
            "tempo_match: starting gRPC tempo adjustment"
        );

        let request = pb::MatchTempoRequest {
            tts_samples: context.audio.samples.clone(),
            tts_sample_rate_hz: context.audio.sample_rate_hz,
            original_timings,
            tts_timings,
            session_id: Some(context.session_id.clone()),
        };

        let mut client = self.client.clone();
        let rpc = client.match_tempo(Request::new(request));
        let response = tokio::time::timeout(self.request_timeout, rpc)
            .await
            .map_err(|_| DomainError::external_service_error("tempo", "gRPC request timed out"))?
            .map_err(|status| map_status("tempo", status))?
            .into_inner();

        tracing::debug!(
            input_samples = context.audio.samples.len(),
            output_samples = response.samples.len(),
            sample_rate_hz = response.sample_rate_hz,
            "tempo_match: gRPC tempo adjustment complete"
        );

        context.audio.samples = response.samples;
        context.audio.sample_rate_hz = response.sample_rate_hz;

        Ok(())
    }
}

pub async fn connect_tempo_client(
    endpoint_uri: &str,
    connect_timeout: Duration,
    max_decoding_message_bytes: usize,
    max_encoding_message_bytes: usize,
) -> Result<TempoServiceClient<Channel>, DomainError> {
    let endpoint = Endpoint::from_shared(endpoint_uri.to_string())
        .map_err(|err| DomainError::internal_error(&format!("invalid tempo endpoint: {err}")))?
        .connect_timeout(connect_timeout);
    let channel = endpoint.connect().await.map_err(|err| {
        DomainError::external_service_error("tempo", &format!("failed to connect: {err}"))
    })?;
    Ok(TempoServiceClient::new(channel)
        .max_decoding_message_size(max_decoding_message_bytes)
        .max_encoding_message_size(max_encoding_message_bytes))
}

fn map_orch_to_proto_timings(words: &[orchestration_domain::WordTiming]) -> Vec<pb::WordTiming> {
    words
        .iter()
        .map(|w| pb::WordTiming {
            word: w.word.clone(),
            start_ms: w.start_ms,
            end_ms: w.end_ms,
            confidence: w.confidence,
        })
        .collect()
}

fn extract_original_timings(
    context: &PipelineContext,
) -> Result<Vec<pb::WordTiming>, DomainError> {
    let timings_value = context
        .extension("original.timings")
        .ok_or_else(|| {
            DomainError::internal_error(
                "tempo_match: extension 'original.timings' not found — was snapshot_original_timings stage run?",
            )
        })?;

    let orch_timings: Vec<orchestration_domain::WordTiming> =
        serde_json::from_value(timings_value.clone()).map_err(|err| {
            DomainError::internal_error(&format!(
                "tempo_match: failed to deserialize original.timings: {err}"
            ))
        })?;

    Ok(map_orch_to_proto_timings(&orch_timings))
}

fn map_status(service: &str, status: tonic::Status) -> DomainError {
    DomainError::external_service_error(
        service,
        &format!("gRPC {}: {}", status.code(), status.message()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchestration_domain::WordTiming;

    #[test]
    fn timing_mapping_preserves_fields() {
        let orch = vec![WordTiming {
            word: "test".to_string(),
            start_ms: 100,
            end_ms: 200,
            confidence: 0.85,
        }];

        let mapped = map_orch_to_proto_timings(&orch);
        assert_eq!(mapped.len(), 1);
        assert_eq!(mapped[0].word, "test");
        assert_eq!(mapped[0].start_ms, 100);
        assert_eq!(mapped[0].end_ms, 200);
        assert_eq!(mapped[0].confidence, 0.85);
    }

    #[test]
    fn extract_timings_fails_without_extension() {
        let context = PipelineContext::new("session", None);
        let result = extract_original_timings(&context);
        assert!(result.is_err());
    }

    #[test]
    fn extract_timings_deserializes_from_extension() {
        let mut context = PipelineContext::new("session", None);
        let words = vec![WordTiming {
            word: "hello".to_string(),
            start_ms: 0,
            end_ms: 500,
            confidence: 0.95,
        }];
        let json = serde_json::to_value(&words).expect("serialize");
        context.set_extension("original.timings", json);

        let result = extract_original_timings(&context).expect("should succeed");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].word, "hello");
    }
}
