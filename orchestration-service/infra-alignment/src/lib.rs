use std::time::Duration;

use alignment_grpc_server::{pb, AlignmentServiceClient};
use async_trait::async_trait;
use orchestration_domain::{
    DomainError, DomainEvent, LanguageTag, PipelineContext, PipelineStage, Transcript,
    TranscriptSegment, TranscriptToken, WordTiming,
};
use serde_json::json;
use tonic::transport::{Channel, Endpoint};
use tonic::Request;

const LANGUAGE_TAG_CODE_FR: i32 = 1;
const LANGUAGE_TAG_CODE_EN: i32 = 2;
const LANGUAGE_TAG_CODE_AUTO: i32 = 3;
const LANGUAGE_TAG_CODE_OTHER: i32 = 4;

pub struct AlignmentEnrichStage {
    client: AlignmentServiceClient<Channel>,
    request_timeout: Duration,
}

impl AlignmentEnrichStage {
    pub fn new(client: AlignmentServiceClient<Channel>, request_timeout: Duration) -> Self {
        Self {
            client,
            request_timeout,
        }
    }
}

#[async_trait]
impl PipelineStage for AlignmentEnrichStage {
    fn name(&self) -> &'static str {
        "alignment_enrich"
    }

    async fn execute(&self, context: &mut PipelineContext) -> Result<(), DomainError> {
        let transcript = context
            .transcript
            .clone()
            .ok_or_else(|| DomainError::internal_error("no transcript available"))?;
        let mut client = self.client.clone();
        let request = pb::EnrichTranscriptRequest {
            samples: context.audio.samples.clone(),
            sample_rate_hz: Some(context.audio.sample_rate_hz),
            transcript: Some(map_transcript_to_proto(transcript)),
            session_id: Some(context.session_id.clone()),
        };
        let rpc = client.enrich_transcript(Request::new(request));
        let response = tokio::time::timeout(self.request_timeout, rpc)
            .await
            .map_err(|_| DomainError::external_service_error("alignment", "gRPC request timed out"))?
            .map_err(|status| map_status("alignment", status))?
            .into_inner();

        let transcript = response
            .transcript
            .ok_or_else(|| DomainError::internal_error("alignment response missing transcript"))
            .and_then(map_transcript_from_proto)?;
        let words = response
            .aligned_words
            .into_iter()
            .map(|word| WordTiming {
                word: word.word,
                start_ms: word.start_ms,
                end_ms: word.end_ms,
                confidence: word.confidence,
            })
            .collect::<Vec<_>>();
        context.session_id = response.session_id;
        context.transcript = Some(transcript);
        context.aligned_words = words.clone();
        context.events.push(DomainEvent::AlignmentUpdate { words });
        context.set_extension("alignment.text", json!(response.text));
        Ok(())
    }
}

pub async fn connect_alignment_client(
    endpoint_uri: &str,
    connect_timeout: Duration,
    max_decoding_message_bytes: usize,
    max_encoding_message_bytes: usize,
) -> Result<AlignmentServiceClient<Channel>, DomainError> {
    let endpoint = Endpoint::from_shared(endpoint_uri.to_string())
        .map_err(|err| DomainError::internal_error(&format!("invalid alignment endpoint: {err}")))?
        .connect_timeout(connect_timeout);
    let channel = endpoint.connect().await.map_err(|err| {
        DomainError::external_service_error("alignment", &format!("failed to connect: {err}"))
    })?;
    Ok(AlignmentServiceClient::new(channel)
        .max_decoding_message_size(max_decoding_message_bytes)
        .max_encoding_message_size(max_encoding_message_bytes))
}

fn map_transcript_to_proto(transcript: Transcript) -> pb::Transcript {
    pb::Transcript {
        language: Some(map_language_to_proto(transcript.language)),
        segments: transcript
            .segments
            .into_iter()
            .map(|segment| pb::TranscriptSegment {
                text: segment.text,
                start_ms: segment.start_ms,
                end_ms: segment.end_ms,
                tokens: segment
                    .tokens
                    .into_iter()
                    .map(|token| pb::TranscriptToken {
                        text: token.text,
                        start_ms: token.start_ms,
                        end_ms: token.end_ms,
                        confidence: token.confidence,
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn map_transcript_from_proto(transcript: pb::Transcript) -> Result<Transcript, DomainError> {
    Ok(Transcript {
        language: map_language_from_proto(transcript.language)?,
        segments: transcript
            .segments
            .into_iter()
            .map(|segment| TranscriptSegment {
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
            })
            .collect(),
    })
}

fn map_language_to_proto(language: LanguageTag) -> pb::LanguageTag {
    match language {
        LanguageTag::Fr => pb::LanguageTag {
            code: LANGUAGE_TAG_CODE_FR,
            other: None,
        },
        LanguageTag::En => pb::LanguageTag {
            code: LANGUAGE_TAG_CODE_EN,
            other: None,
        },
        LanguageTag::Auto => pb::LanguageTag {
            code: LANGUAGE_TAG_CODE_AUTO,
            other: None,
        },
        LanguageTag::Other(value) => pb::LanguageTag {
            code: LANGUAGE_TAG_CODE_OTHER,
            other: Some(value),
        },
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
    fn language_mapping_round_trips() {
        let tag = LanguageTag::Other("es".to_string());
        let proto = map_language_to_proto(tag.clone());
        let mapped = map_language_from_proto(Some(proto)).expect("language should map");
        match mapped {
            LanguageTag::Other(value) => assert_eq!(value, "es"),
            _ => panic!("expected Other language"),
        }
    }

    #[test]
    fn transcript_round_trip_preserves_tokens() {
        let transcript = Transcript {
            language: LanguageTag::En,
            segments: vec![TranscriptSegment {
                text: "hello".to_string(),
                start_ms: 0,
                end_ms: 100,
                tokens: vec![TranscriptToken {
                    text: "hello".to_string(),
                    start_ms: 0,
                    end_ms: 100,
                    confidence: 0.9,
                }],
            }],
        };

        let mapped = map_transcript_from_proto(map_transcript_to_proto(transcript.clone()))
            .expect("transcript should map");
        assert_eq!(mapped.segments.len(), 1);
        assert_eq!(mapped.segments[0].tokens.len(), 1);
        assert_eq!(mapped.segments[0].tokens[0].text, "hello");
        assert_eq!(mapped.segments[0].tokens[0].confidence, 0.9);
    }
}
