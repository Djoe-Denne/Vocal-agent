use std::{
    net::{SocketAddr, ToSocketAddrs},
    sync::Arc,
};

use anyhow::Context;
use audio_application::{TransformAudioCommand, TransformAudioRequest, TransformAudioResponse};
use audio_domain::TransformMetadata;
use rustycog_command::{CommandContext, CommandError, GenericCommandService};
use rustycog_config::ServerConfig;
use tonic::{transport::Server, Request, Response, Status};

const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;

pub mod pb {
    tonic::include_proto!("audio.v1");
}

pub use pb::audio_service_client::AudioServiceClient;
pub use pb::audio_service_server::AudioServiceServer;

pub async fn serve_grpc(
    command_service: Arc<GenericCommandService>,
    server_config: ServerConfig,
) -> anyhow::Result<()> {
    let address = resolve_bind_addr(&server_config)?;
    let service = AudioGrpcService { command_service };

    tracing::info!(
        host = %server_config.host,
        port = server_config.port,
        "starting audio gRPC server"
    );

    Server::builder()
        .add_service(
            AudioServiceServer::new(service)
                .max_decoding_message_size(MAX_MESSAGE_BYTES)
                .max_encoding_message_size(MAX_MESSAGE_BYTES),
        )
        .serve(address)
        .await
        .context("audio gRPC server failed")
}

#[derive(Clone)]
struct AudioGrpcService {
    command_service: Arc<GenericCommandService>,
}

#[tonic::async_trait]
impl pb::audio_service_server::AudioService for AudioGrpcService {
    async fn transform_audio(
        &self,
        request: Request<pb::TransformAudioRequest>,
    ) -> Result<Response<pb::TransformAudioResponse>, Status> {
        let request = map_transform_request(request.into_inner())?;
        let command = TransformAudioCommand::new(request);
        let context = CommandContext::new();
        let result = self
            .command_service
            .execute(command, context)
            .await
            .map_err(map_command_error)?;

        Ok(Response::new(map_transform_response(result)))
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

fn map_transform_request(request: pb::TransformAudioRequest) -> Result<TransformAudioRequest, Status> {
    if request.samples.is_empty() {
        return Err(Status::invalid_argument(
            "samples must contain at least one frame",
        ));
    }

    validate_sample_rate(request.sample_rate_hz, "sample_rate_hz")?;
    validate_sample_rate(request.target_sample_rate_hz, "target_sample_rate_hz")?;
    validate_optional_text(&request.session_id, "session_id", 64)?;

    Ok(TransformAudioRequest {
        samples: request.samples,
        sample_rate_hz: request.sample_rate_hz,
        target_sample_rate_hz: request.target_sample_rate_hz,
        session_id: request.session_id,
    })
}

fn map_transform_response(response: TransformAudioResponse) -> pb::TransformAudioResponse {
    pb::TransformAudioResponse {
        session_id: response.session_id,
        samples: response.samples,
        sample_rate_hz: response.sample_rate_hz,
        metadata: Some(map_transform_metadata(response.metadata)),
    }
}

fn map_transform_metadata(metadata: TransformMetadata) -> pb::TransformMetadata {
    pb::TransformMetadata {
        clamped: metadata.clamped,
        resampled: metadata.resampled,
        input_sample_count: metadata.input_sample_count as u64,
        output_sample_count: metadata.output_sample_count as u64,
        source_sample_rate_hz: metadata.source_sample_rate_hz,
        target_sample_rate_hz: metadata.target_sample_rate_hz,
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

fn validate_sample_rate(value: Option<u32>, field: &str) -> Result<(), Status> {
    if let Some(sample_rate_hz) = value {
        if !(8_000..=192_000).contains(&sample_rate_hz) {
            return Err(Status::invalid_argument(format!(
                "{field} must be between 8000 and 192000"
            )));
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

    use audio_application::{AudioCommandRegistryFactory, TransformAudioUseCase};
    use audio_domain::TransformMetadata;
    use rustycog_command::GenericCommandService;
    use rustycog_config::ServerConfig;
    use tonic::Request;

    use super::{pb, serve_grpc, AudioServiceClient};

    struct MockAudioUseCase;

    #[tonic::async_trait]
    impl TransformAudioUseCase for MockAudioUseCase {
        async fn transform_audio(
            &self,
            request: audio_application::TransformAudioRequest,
        ) -> Result<audio_application::TransformAudioResponse, audio_application::ApplicationError>
        {
            Ok(audio_application::TransformAudioResponse {
                session_id: request
                    .session_id
                    .unwrap_or_else(|| "generated-session".to_string()),
                samples: vec![0.11, 0.22, 0.33],
                sample_rate_hz: request.target_sample_rate_hz.unwrap_or(16_000),
                metadata: TransformMetadata {
                    clamped: false,
                    resampled: true,
                    input_sample_count: request.samples.len(),
                    output_sample_count: 3,
                    source_sample_rate_hz: request.sample_rate_hz.unwrap_or(16_000),
                    target_sample_rate_hz: request.target_sample_rate_hz.unwrap_or(16_000),
                },
            })
        }
    }

    #[tokio::test]
    async fn transform_audio_rpc_smoke() {
        let port = pick_free_port();
        let mut server_config = ServerConfig::default();
        server_config.host = "127.0.0.1".to_string();
        server_config.port = port;

        let registry = AudioCommandRegistryFactory::create_registry(Arc::new(MockAudioUseCase));
        let command_service = Arc::new(GenericCommandService::new(Arc::new(registry)));

        let server = tokio::spawn(async move { serve_grpc(command_service, server_config).await });
        let endpoint = format!("http://127.0.0.1:{port}");
        let mut client = connect_with_retry(endpoint).await;

        let response = client
            .transform_audio(Request::new(pb::TransformAudioRequest {
                samples: vec![0.1, 0.2, 0.3],
                sample_rate_hz: Some(48_000),
                target_sample_rate_hz: Some(16_000),
                session_id: Some("it-session".to_string()),
            }))
            .await
            .expect("rpc succeeds")
            .into_inner();

        assert_eq!(response.session_id, "it-session");
        assert_eq!(response.sample_rate_hz, 16_000);
        assert_eq!(response.samples.len(), 3);
        assert!(response.metadata.expect("metadata").resampled);

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

    async fn connect_with_retry(endpoint: String) -> AudioServiceClient<tonic::transport::Channel> {
        for _ in 0..40 {
            if let Ok(client) = AudioServiceClient::connect(endpoint.clone()).await {
                return client;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("unable to connect gRPC client to {endpoint}");
    }
}
