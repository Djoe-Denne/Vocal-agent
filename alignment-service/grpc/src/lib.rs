use std::{
    net::{SocketAddr, ToSocketAddrs},
    sync::Arc,
};

use anyhow::Context;
use alignment_application::{
    EnrichTranscriptCommand, EnrichTranscriptRequest, EnrichTranscriptResponse,
};
use alignment_domain::{LanguageTag, Transcript, TranscriptSegment, TranscriptToken, WordTiming};
use rustycog_command::{CommandContext, CommandError, GenericCommandService};
use rustycog_config::ServerConfig;
use tonic::{transport::Server, Request, Response, Status};

const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;
const LANGUAGE_TAG_CODE_FR: i32 = 1;
const LANGUAGE_TAG_CODE_EN: i32 = 2;
const LANGUAGE_TAG_CODE_AUTO: i32 = 3;
const LANGUAGE_TAG_CODE_OTHER: i32 = 4;

pub mod pb {
    tonic::include_proto!("alignment.v1");
}

pub use pb::alignment_service_client::AlignmentServiceClient;
pub use pb::alignment_service_server::AlignmentServiceServer;

pub async fn serve_grpc(
    command_service: Arc<GenericCommandService>,
    server_config: ServerConfig,
) -> anyhow::Result<()> {
    let address = resolve_bind_addr(&server_config)?;
    let service = AlignmentGrpcService { command_service };

    tracing::info!(
        host = %server_config.host,
        port = server_config.port,
        "starting alignment gRPC server"
    );

    Server::builder()
        .add_service(
            AlignmentServiceServer::new(service)
                .max_decoding_message_size(MAX_MESSAGE_BYTES)
                .max_encoding_message_size(MAX_MESSAGE_BYTES),
        )
        .serve(address)
        .await
        .context("alignment gRPC server failed")
}

#[derive(Clone)]
struct AlignmentGrpcService {
    command_service: Arc<GenericCommandService>,
}

#[tonic::async_trait]
impl pb::alignment_service_server::AlignmentService for AlignmentGrpcService {
    async fn enrich_transcript(
        &self,
        request: Request<pb::EnrichTranscriptRequest>,
    ) -> Result<Response<pb::EnrichTranscriptResponse>, Status> {
        let request = map_enrich_request(request.into_inner())?;
        let command = EnrichTranscriptCommand::new(request);
        let context = CommandContext::new();
        let result = self
            .command_service
            .execute(command, context)
            .await
            .map_err(map_command_error)?;

        Ok(Response::new(map_enrich_response(result)))
    }
}

fn resolve_bind_addr(config: &ServerConfig) -> anyhow::Result<SocketAddr> {
    let bind = format!("{}:{}", config.host, config.port);
    let mut resolved = bind
        .to_socket_addrs()
        .with_context(|| format!("invalid gRPC bind address `{bind}`"))?;

    resolved
        .next()
        .with_context(|| format!("no socket address resolved for `{bind}`"))
}

fn map_enrich_request(request: pb::EnrichTranscriptRequest) -> Result<EnrichTranscriptRequest, Status> {
    if request.samples.is_empty() {
        return Err(Status::invalid_argument(
            "samples must contain at least one frame",
        ));
    }

    validate_sample_rate(request.sample_rate_hz)?;
    validate_optional_text(&request.session_id, "session_id", 64)?;
    let transcript = map_transcript_from_proto(request.transcript)?;

    Ok(EnrichTranscriptRequest {
        samples: request.samples,
        sample_rate_hz: request.sample_rate_hz,
        transcript,
        session_id: request.session_id,
    })
}

fn map_enrich_response(response: EnrichTranscriptResponse) -> pb::EnrichTranscriptResponse {
    pb::EnrichTranscriptResponse {
        session_id: response.session_id,
        transcript: Some(map_transcript(response.transcript)),
        aligned_words: response
            .aligned_words
            .into_iter()
            .map(map_word_timing)
            .collect(),
        text: response.text,
    }
}

fn map_transcript(transcript: Transcript) -> pb::Transcript {
    pb::Transcript {
        language: Some(map_language_tag(transcript.language)),
        segments: transcript
            .segments
            .into_iter()
            .map(map_transcript_segment)
            .collect(),
    }
}

fn map_transcript_segment(segment: TranscriptSegment) -> pb::TranscriptSegment {
    pb::TranscriptSegment {
        text: segment.text,
        start_ms: segment.start_ms,
        end_ms: segment.end_ms,
        tokens: segment.tokens.into_iter().map(map_transcript_token).collect(),
    }
}

fn map_transcript_token(token: TranscriptToken) -> pb::TranscriptToken {
    pb::TranscriptToken {
        text: token.text,
        start_ms: token.start_ms,
        end_ms: token.end_ms,
        confidence: token.confidence,
    }
}

fn map_word_timing(word: WordTiming) -> pb::WordTiming {
    pb::WordTiming {
        word: word.word,
        start_ms: word.start_ms,
        end_ms: word.end_ms,
        confidence: word.confidence,
    }
}

fn map_transcript_from_proto(transcript: Option<pb::Transcript>) -> Result<Transcript, Status> {
    let transcript = transcript.ok_or_else(|| Status::invalid_argument("transcript is required"))?;

    Ok(Transcript {
        language: map_language_tag_from_proto(transcript.language)?,
        segments: transcript
            .segments
            .into_iter()
            .map(map_transcript_segment_from_proto)
            .collect(),
    })
}

fn map_transcript_segment_from_proto(segment: pb::TranscriptSegment) -> TranscriptSegment {
    TranscriptSegment {
        text: segment.text,
        start_ms: segment.start_ms,
        end_ms: segment.end_ms,
        tokens: segment
            .tokens
            .into_iter()
            .map(map_transcript_token_from_proto)
            .collect(),
    }
}

fn map_transcript_token_from_proto(token: pb::TranscriptToken) -> TranscriptToken {
    TranscriptToken {
        text: token.text,
        start_ms: token.start_ms,
        end_ms: token.end_ms,
        confidence: token.confidence,
    }
}

fn map_language_tag(language: LanguageTag) -> pb::LanguageTag {
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

fn map_language_tag_from_proto(language: Option<pb::LanguageTag>) -> Result<LanguageTag, Status> {
    let language = language.ok_or_else(|| Status::invalid_argument("transcript.language is required"))?;

    match language.code {
        LANGUAGE_TAG_CODE_FR => Ok(LanguageTag::Fr),
        LANGUAGE_TAG_CODE_EN => Ok(LanguageTag::En),
        LANGUAGE_TAG_CODE_AUTO => Ok(LanguageTag::Auto),
        LANGUAGE_TAG_CODE_OTHER => {
            let value = language.other.unwrap_or_default();
            if value.is_empty() {
                return Err(Status::invalid_argument(
                    "transcript.language.other cannot be empty when code is OTHER",
                ));
            }
            Ok(LanguageTag::Other(value))
        }
        _ => Err(Status::invalid_argument(
            "transcript.language.code is invalid",
        )),
    }
}

fn map_command_error(error: CommandError) -> Status {
    match error {
        CommandError::Validation { .. } => Status::invalid_argument(error.to_string()),
        CommandError::Authentication { .. } => Status::unauthenticated(error.to_string()),
        CommandError::Business { .. } => {
            let message = error.message().to_ascii_lowercase();
            if message.contains("not found") {
                Status::not_found(error.to_string())
            } else if message.contains("permission") || message.contains("forbidden") {
                Status::permission_denied(error.to_string())
            } else {
                Status::failed_precondition(error.to_string())
            }
        }
        CommandError::Infrastructure { .. }
        | CommandError::Timeout { .. }
        | CommandError::RetryExhausted { .. } => Status::internal(error.to_string()),
    }
}

fn validate_sample_rate(value: Option<u32>) -> Result<(), Status> {
    if let Some(sample_rate_hz) = value {
        if !(8_000..=192_000).contains(&sample_rate_hz) {
            return Err(Status::invalid_argument(
                "sample_rate_hz must be between 8000 and 192000",
            ));
        }
    }

    Ok(())
}

fn validate_optional_text(value: &Option<String>, field: &str, max_len: usize) -> Result<(), Status> {
    if let Some(text) = value {
        if text.is_empty() {
            return Err(Status::invalid_argument(format!(
                "{field} cannot be empty"
            )));
        }
        if text.len() > max_len {
            return Err(Status::invalid_argument(format!(
                "{field} must be <= {max_len} chars"
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{net::TcpListener, sync::Arc, time::Duration};

    use alignment_application::{AlignTranscriptUseCase, AlignmentCommandRegistryFactory};
    use alignment_domain::WordTiming;
    use rustycog_command::GenericCommandService;
    use rustycog_config::ServerConfig;
    use tonic::Request;

    use super::{pb, serve_grpc, AlignmentServiceClient};

    struct MockAlignmentUseCase;

    #[tonic::async_trait]
    impl AlignTranscriptUseCase for MockAlignmentUseCase {
        async fn enrich_transcript(
            &self,
            request: alignment_application::EnrichTranscriptRequest,
        ) -> Result<
            alignment_application::EnrichTranscriptResponse,
            alignment_application::ApplicationError,
        > {
            Ok(alignment_application::EnrichTranscriptResponse {
                session_id: request
                    .session_id
                    .unwrap_or_else(|| "generated-session".to_string()),
                transcript: request.transcript,
                aligned_words: vec![WordTiming {
                    word: "hello".to_string(),
                    start_ms: 0,
                    end_ms: 150,
                    confidence: 0.95,
                }],
                text: "hello world".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn enrich_transcript_rpc_smoke() {
        let port = pick_free_port();
        let mut server_config = ServerConfig::default();
        server_config.host = "127.0.0.1".to_string();
        server_config.port = port;

        let registry =
            AlignmentCommandRegistryFactory::create_registry(Arc::new(MockAlignmentUseCase));
        let command_service = Arc::new(GenericCommandService::new(Arc::new(registry)));

        let server = tokio::spawn(async move { serve_grpc(command_service, server_config).await });
        let endpoint = format!("http://127.0.0.1:{port}");
        let mut client = connect_with_retry(endpoint).await;

        let response = client
            .enrich_transcript(Request::new(pb::EnrichTranscriptRequest {
                samples: vec![0.1, 0.2, 0.3],
                sample_rate_hz: Some(16_000),
                transcript: Some(pb::Transcript {
                    language: Some(pb::LanguageTag {
                        code: super::LANGUAGE_TAG_CODE_EN,
                        other: None,
                    }),
                    segments: vec![pb::TranscriptSegment {
                        text: "hello world".to_string(),
                        start_ms: 0,
                        end_ms: 250,
                        tokens: vec![],
                    }],
                }),
                session_id: Some("it-session".to_string()),
            }))
            .await
            .expect("rpc succeeds")
            .into_inner();

        assert_eq!(response.session_id, "it-session");
        assert_eq!(response.text, "hello world");
        assert_eq!(response.aligned_words.len(), 1);

        server.abort();
        let _ = server.await;
    }

    fn pick_free_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .expect("bind ephemeral port")
            .local_addr()
            .expect("extract local addr")
            .port()
    }

    async fn connect_with_retry(
        endpoint: String,
    ) -> AlignmentServiceClient<tonic::transport::Channel> {
        for _ in 0..40 {
            if let Ok(client) = AlignmentServiceClient::connect(endpoint.clone()).await {
                return client;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("unable to connect gRPC client to {endpoint}");
    }

}
