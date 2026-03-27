use std::{
    net::{SocketAddr, ToSocketAddrs},
    sync::Arc,
};

use anyhow::Context;
use tempo_application::{MatchTempoCommand, MatchTempoRequest, MatchTempoResponse};
use tempo_domain::WordTiming;
use rustycog_command::{CommandContext, CommandError, GenericCommandService};
use rustycog_config::ServerConfig;
use tonic::{transport::Server, Request, Response, Status};

const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;

pub mod pb {
    tonic::include_proto!("tempo.v1");
}

pub use pb::tempo_service_client::TempoServiceClient;
pub use pb::tempo_service_server::TempoServiceServer;

pub async fn serve_grpc(
    command_service: Arc<GenericCommandService>,
    server_config: ServerConfig,
) -> anyhow::Result<()> {
    let address = resolve_bind_addr(&server_config)?;
    let service = TempoGrpcService { command_service };

    tracing::info!(
        host = %server_config.host,
        port = server_config.port,
        "starting tempo gRPC server"
    );

    Server::builder()
        .add_service(
            TempoServiceServer::new(service)
                .max_decoding_message_size(MAX_MESSAGE_BYTES)
                .max_encoding_message_size(MAX_MESSAGE_BYTES),
        )
        .serve(address)
        .await
        .context("tempo gRPC server failed")
}

#[derive(Clone)]
struct TempoGrpcService {
    command_service: Arc<GenericCommandService>,
}

#[tonic::async_trait]
impl pb::tempo_service_server::TempoService for TempoGrpcService {
    async fn match_tempo(
        &self,
        request: Request<pb::MatchTempoRequest>,
    ) -> Result<Response<pb::MatchTempoResponse>, Status> {
        let request = map_match_request(request.into_inner())?;
        let command = MatchTempoCommand::new(request);
        let context = CommandContext::new();
        let result = self
            .command_service
            .execute(command, context)
            .await
            .map_err(map_command_error)?;

        Ok(Response::new(map_match_response(result)))
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

fn map_match_request(request: pb::MatchTempoRequest) -> Result<MatchTempoRequest, Status> {
    if request.tts_samples.is_empty() {
        return Err(Status::invalid_argument(
            "tts_samples must contain at least one frame",
        ));
    }

    if request.original_timings.is_empty() {
        return Err(Status::invalid_argument(
            "original_timings must contain at least one word",
        ));
    }

    if request.tts_timings.is_empty() {
        return Err(Status::invalid_argument(
            "tts_timings must contain at least one word",
        ));
    }

    validate_optional_text(&request.session_id, "session_id", 64)?;

    Ok(MatchTempoRequest {
        tts_samples: request.tts_samples,
        tts_sample_rate_hz: Some(request.tts_sample_rate_hz),
        original_timings: request
            .original_timings
            .into_iter()
            .map(map_word_timing_from_proto)
            .collect(),
        tts_timings: request
            .tts_timings
            .into_iter()
            .map(map_word_timing_from_proto)
            .collect(),
        session_id: request.session_id,
    })
}

fn map_match_response(response: MatchTempoResponse) -> pb::MatchTempoResponse {
    pb::MatchTempoResponse {
        session_id: response.session_id,
        samples: response.samples,
        sample_rate_hz: response.sample_rate_hz,
    }
}

fn map_word_timing_from_proto(word: pb::WordTiming) -> WordTiming {
    WordTiming {
        word: word.word,
        start_ms: word.start_ms,
        end_ms: word.end_ms,
        confidence: word.confidence,
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

fn validate_optional_text(
    value: &Option<String>,
    field: &str,
    max_len: usize,
) -> Result<(), Status> {
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
