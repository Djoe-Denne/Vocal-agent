use std::time::Duration;

use async_trait::async_trait;
use asr_grpc_server::{pb, AsrServiceClient};
use orchestration_domain::{
    DomainError, DomainEvent, LanguageTag, PipelineContext, PipelineStage, Transcript,
    TranscriptSegment, TranscriptToken,
};
use serde_json::json;
use tonic::transport::{Channel, Endpoint};
use tonic::Request;

const LANGUAGE_TAG_CODE_FR: i32 = 1;
const LANGUAGE_TAG_CODE_EN: i32 = 2;
const LANGUAGE_TAG_CODE_AUTO: i32 = 3;
const LANGUAGE_TAG_CODE_OTHER: i32 = 4;

pub struct AsrTranscribeStage {
    client: AsrServiceClient<Channel>,
    request_timeout: Duration,
}

impl AsrTranscribeStage {
    pub fn new(client: AsrServiceClient<Channel>, request_timeout: Duration) -> Self {
        Self {
            client,
            request_timeout,
        }
    }
}

#[async_trait]
impl PipelineStage for AsrTranscribeStage {
    fn name(&self) -> &'static str {
        "asr_transcribe"
    }

    async fn execute(&self, context: &mut PipelineContext) -> Result<(), DomainError> {
        let mut client = self.client.clone();
        let request = pb::TranscribeAudioRequest {
            samples: context.audio.samples.clone(),
            sample_rate_hz: Some(context.audio.sample_rate_hz),
            language_hint: context.language_hint.as_ref().map(language_hint),
            session_id: Some(context.session_id.clone()),
        };
        let rpc = client.transcribe(Request::new(request));
        let response = tokio::time::timeout(self.request_timeout, rpc)
            .await
            .map_err(|_| DomainError::external_service_error("asr", "gRPC request timed out"))?
            .map_err(|status| map_status("asr", status))?
            .into_inner();

        let transcript = response
            .transcript
            .ok_or_else(|| DomainError::internal_error("asr response missing transcript"))
            .and_then(map_transcript_from_proto)?;
        context.session_id = response.session_id;
        context.transcript = Some(transcript.clone());
        context.events.push(DomainEvent::FinalTranscript { transcript });
        context.set_extension("asr.text", json!(response.text));
        Ok(())
    }
}

pub async fn connect_asr_client(
    endpoint_uri: &str,
    connect_timeout: Duration,
    max_decoding_message_bytes: usize,
    max_encoding_message_bytes: usize,
) -> Result<AsrServiceClient<Channel>, DomainError> {
    let endpoint = Endpoint::from_shared(endpoint_uri.to_string())
        .map_err(|err| DomainError::internal_error(&format!("invalid asr endpoint: {err}")))?
        .connect_timeout(connect_timeout);
    let channel = endpoint
        .connect()
        .await
        .map_err(|err| DomainError::external_service_error("asr", &format!("failed to connect: {err}")))?;
    Ok(AsrServiceClient::new(channel)
        .max_decoding_message_size(max_decoding_message_bytes)
        .max_encoding_message_size(max_encoding_message_bytes))
}

fn map_transcript_from_proto(transcript: pb::Transcript) -> Result<Transcript, DomainError> {
    Ok(Transcript {
        language: map_language_from_proto(transcript.language)?,
        segments: transcript
            .segments
            .into_iter()
            .map(map_segment_from_proto)
            .collect(),
    })
}

fn map_segment_from_proto(segment: pb::TranscriptSegment) -> TranscriptSegment {
    TranscriptSegment {
        text: segment.text,
        start_ms: segment.start_ms,
        end_ms: segment.end_ms,
        tokens: segment
            .tokens
            .into_iter()
            .map(|token| TranscriptToken {
                text: token.text,
                start_ms: token.start_ms,
                end_ms: token.end_ms,
                confidence: token.confidence,
            })
            .collect(),
    }
}

fn map_language_from_proto(language: Option<pb::LanguageTag>) -> Result<LanguageTag, DomainError> {
    let language = language.ok_or_else(|| DomainError::internal_error("missing language tag"))?;
    match language.code {
        LANGUAGE_TAG_CODE_FR => Ok(LanguageTag::Fr),
        LANGUAGE_TAG_CODE_EN => Ok(LanguageTag::En),
        LANGUAGE_TAG_CODE_AUTO => Ok(LanguageTag::Auto),
        LANGUAGE_TAG_CODE_OTHER => {
            let value = language.other.unwrap_or_default();
            if value.trim().is_empty() {
                return Err(DomainError::internal_error(
                    "language.other is required when code is OTHER",
                ));
            }
            Ok(LanguageTag::Other(value))
        }
        _ => Err(DomainError::internal_error("invalid language tag code")),
    }
}

fn language_hint(tag: &LanguageTag) -> String {
    match tag {
        LanguageTag::Fr => "fr".to_string(),
        LanguageTag::En => "en".to_string(),
        LanguageTag::Auto => "auto".to_string(),
        LanguageTag::Other(value) => value.clone(),
    }
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

    #[test]
    fn language_hint_maps_known_tags() {
        assert_eq!(language_hint(&LanguageTag::Fr), "fr");
        assert_eq!(language_hint(&LanguageTag::En), "en");
        assert_eq!(language_hint(&LanguageTag::Auto), "auto");
        assert_eq!(
            language_hint(&LanguageTag::Other("de".to_string())),
            "de".to_string()
        );
    }

    #[test]
    fn transcript_mapping_preserves_segments_and_tokens() {
        let mapped = map_transcript_from_proto(pb::Transcript {
            language: Some(pb::LanguageTag {
                code: LANGUAGE_TAG_CODE_EN,
                other: None,
            }),
            segments: vec![pb::TranscriptSegment {
                text: "hello".to_string(),
                start_ms: 10,
                end_ms: 50,
                tokens: vec![pb::TranscriptToken {
                    text: "hello".to_string(),
                    start_ms: 10,
                    end_ms: 50,
                    confidence: 0.95,
                }],
            }],
        })
        .expect("mapping should succeed");

        assert_eq!(mapped.segments.len(), 1);
        assert_eq!(mapped.segments[0].tokens.len(), 1);
        assert_eq!(mapped.segments[0].tokens[0].text, "hello");
        assert_eq!(mapped.segments[0].tokens[0].confidence, 0.95);
    }

    #[test]
    fn language_other_requires_value() {
        let error = map_language_from_proto(Some(pb::LanguageTag {
            code: LANGUAGE_TAG_CODE_OTHER,
            other: None,
        }))
        .expect_err("mapping should fail without other value");

        assert!(error.to_string().contains("language.other"));
    }
}
