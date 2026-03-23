use std::time::Duration;

use async_trait::async_trait;
use orchestration_domain::{
    DomainError, PipelineContext, PipelineStage, TtsOutput, WordTiming,
};
use serde_json::json;
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
        let original_timings = deserialize_original_timings(context)?;
        let tts_timings: Vec<pb::WordTiming> = context
            .aligned_words
            .iter()
            .map(|w| pb::WordTiming {
                word: w.word.clone(),
                start_ms: w.start_ms,
                end_ms: w.end_ms,
                confidence: w.confidence,
            })
            .collect();

        if tts_timings.is_empty() {
            return Err(DomainError::internal_error(
                "tempo_match requires TTS aligned_words from re-alignment",
            ));
        }

        let mut client = self.client.clone();
        let request = pb::MatchTempoRequest {
            tts_samples: context.audio.samples.clone(),
            tts_sample_rate_hz: context.audio.sample_rate_hz,
            original_timings,
            tts_timings,
            session_id: Some(context.session_id.clone()),
        };

        let rpc = client.match_tempo(Request::new(request));
        let response = tokio::time::timeout(self.request_timeout, rpc)
            .await
            .map_err(|_| DomainError::external_service_error("tempo", "gRPC request timed out"))?
            .map_err(|status| map_status("tempo", status))?
            .into_inner();

        context.session_id = response.session_id;
        context.audio.samples = response.samples.clone();
        context.audio.sample_rate_hz = response.sample_rate_hz;
        context.set_extension("tempo.output_sample_count", json!(response.samples.len()));
        context.set_extension("tempo.sample_rate_hz", json!(response.sample_rate_hz));

        if let Some(ref mut tts_output) = context.tts_output {
            tts_output.samples = response.samples;
            tts_output.sample_rate_hz = response.sample_rate_hz;
        } else {
            context.tts_output = Some(TtsOutput {
                samples: response.samples,
                sample_rate_hz: response.sample_rate_hz,
                word_timings: vec![],
            });
        }

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
        .map_err(|err| {
            DomainError::internal_error(&format!("invalid tempo endpoint: {err}"))
        })?
        .connect_timeout(connect_timeout);
    let channel = endpoint.connect().await.map_err(|err| {
        DomainError::external_service_error("tempo", &format!("failed to connect: {err}"))
    })?;
    Ok(TempoServiceClient::new(channel)
        .max_decoding_message_size(max_decoding_message_bytes)
        .max_encoding_message_size(max_encoding_message_bytes))
}

fn deserialize_original_timings(
    context: &PipelineContext,
) -> Result<Vec<pb::WordTiming>, DomainError> {
    let timings_value = context
        .extension("original.timings")
        .ok_or_else(|| {
            DomainError::internal_error(
                "tempo_match requires original.timings snapshot in extensions",
            )
        })?;

    let timings: Vec<WordTiming> =
        serde_json::from_value(timings_value.clone()).map_err(|err| {
            DomainError::internal_error(&format!(
                "failed to deserialize original.timings: {err}"
            ))
        })?;

    Ok(timings
        .into_iter()
        .map(|w| pb::WordTiming {
            word: w.word,
            start_ms: w.start_ms,
            end_ms: w.end_ms,
            confidence: w.confidence,
        })
        .collect())
}

fn map_status(service: &str, status: tonic::Status) -> DomainError {
    DomainError::external_service_error(
        service,
        &format!("gRPC {}: {}", status.code(), status.message()),
    )
}

#[cfg(test)]
mod tests {
    use orchestration_domain::{PipelineContext, WordTiming};
    use serde_json::json;

    use super::deserialize_original_timings;

    #[test]
    fn deserialize_original_timings_from_context() {
        let mut context = PipelineContext::new("session", None);
        let timings = vec![WordTiming {
            word: "hello".to_string(),
            start_ms: 0,
            end_ms: 500,
            confidence: 0.95,
        }];
        context.set_extension(
            "original.timings",
            serde_json::to_value(&timings).unwrap(),
        );

        let result = deserialize_original_timings(&context).expect("should deserialize");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].word, "hello");
        assert_eq!(result[0].start_ms, 0);
        assert_eq!(result[0].end_ms, 500);
    }

    #[test]
    fn deserialize_fails_when_missing() {
        let context = PipelineContext::new("session", None);
        let result = deserialize_original_timings(&context);
        assert!(result.is_err());
    }
}
