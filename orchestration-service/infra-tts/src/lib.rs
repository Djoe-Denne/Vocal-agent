use std::time::Duration;

use async_trait::async_trait;
use orchestration_domain::{
    DomainError, PipelineContext, PipelineStage, SynthesizedWordTiming, TtsOutput, WordTiming,
};
use serde_json::json;
use tonic::transport::{Channel, Endpoint};
use tonic::Request;
use tts_grpc_server::{pb, TtsServiceClient};

pub struct TtsSynthesizeStage {
    client: TtsServiceClient<Channel>,
    request_timeout: Duration,
}

impl TtsSynthesizeStage {
    pub fn new(client: TtsServiceClient<Channel>, request_timeout: Duration) -> Self {
        Self {
            client,
            request_timeout,
        }
    }
}

#[async_trait]
impl PipelineStage for TtsSynthesizeStage {
    fn name(&self) -> &'static str {
        "tts_synthesize"
    }

    async fn execute(&self, context: &mut PipelineContext) -> Result<(), DomainError> {
        let transcript = build_timed_transcript(context)?;
        let mut client = self.client.clone();
        let request = pb::SynthesizeAudioRequest {
            transcript: Some(transcript),
            prosody: vec![],
            sample_rate_hz: None,
            session_id: Some(context.session_id.clone()),
        };

        let rpc = client.synthesize_audio(Request::new(request));
        let response = tokio::time::timeout(self.request_timeout, rpc)
            .await
            .map_err(|_| DomainError::external_service_error("tts", "gRPC request timed out"))?
            .map_err(|status| map_status("tts", status))?
            .into_inner();

        let word_timings = response
            .word_timings
            .into_iter()
            .map(|word| SynthesizedWordTiming {
                text: word.text,
                start_ms: word.start_ms,
                end_ms: word.end_ms,
                fit_strategy: word.fit_strategy,
            })
            .collect::<Vec<_>>();

        let tts_output = TtsOutput {
            samples: response.samples,
            sample_rate_hz: response.sample_rate_hz,
            word_timings,
        };

        context.session_id = response.session_id;
        context.set_extension("tts.sample_count", json!(tts_output.samples.len()));
        context.set_extension("tts.sample_rate_hz", json!(tts_output.sample_rate_hz));
        context.tts_output = Some(tts_output);
        Ok(())
    }
}

pub async fn connect_tts_client(
    endpoint_uri: &str,
    connect_timeout: Duration,
    max_decoding_message_bytes: usize,
    max_encoding_message_bytes: usize,
) -> Result<TtsServiceClient<Channel>, DomainError> {
    let endpoint = Endpoint::from_shared(endpoint_uri.to_string())
        .map_err(|err| DomainError::internal_error(&format!("invalid tts endpoint: {err}")))?
        .connect_timeout(connect_timeout);
    let channel = endpoint
        .connect()
        .await
        .map_err(|err| DomainError::external_service_error("tts", &format!("failed to connect: {err}")))?;
    Ok(TtsServiceClient::new(channel)
        .max_decoding_message_size(max_decoding_message_bytes)
        .max_encoding_message_size(max_encoding_message_bytes))
}

fn build_timed_transcript(context: &PipelineContext) -> Result<pb::TimedTranscript, DomainError> {
    let transcript = context
        .transcript
        .as_ref()
        .ok_or_else(|| DomainError::internal_error("no transcript available"))?;

    let words = if !context.aligned_words.is_empty() {
        context
            .aligned_words
            .iter()
            .map(map_word_timing)
            .collect::<Vec<_>>()
    } else {
        transcript
            .segments
            .iter()
            .flat_map(|segment| segment.tokens.iter())
            .map(|token| pb::TimedWord {
                text: token.text.clone(),
                start_ms: token.start_ms,
                end_ms: token.end_ms,
            })
            .collect::<Vec<_>>()
    };

    if words.is_empty() {
        return Err(DomainError::internal_error(
            "tts requires aligned words or token timings",
        ));
    }

    let total_duration_ms = words.iter().map(|word| word.end_ms).max().unwrap_or(0);
    Ok(pb::TimedTranscript {
        total_duration_ms,
        words,
    })
}

fn map_word_timing(word: &WordTiming) -> pb::TimedWord {
    pb::TimedWord {
        text: word.word.clone(),
        start_ms: word.start_ms,
        end_ms: word.end_ms,
    }
}

fn map_status(service: &str, status: tonic::Status) -> DomainError {
    DomainError::external_service_error(
        service,
        &format!("gRPC {}: {}", status.code(), status.message()),
    )
}

