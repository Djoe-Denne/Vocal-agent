use std::{future::Future, sync::Arc, time::Duration};

use anyhow::{anyhow, Error};
use orchestration_application::{
    AsrCommandRegistryFactory, AsrUseCase, AsrUseCaseImpl, PipelineDefinition, PipelineEngine,
    PipelineStepLoader, PipelineStepSpec,
};
use orchestration_configuration::{AppConfig, GrpcEndpointConfig, PipelineDefinitionConfig};
use orchestration_domain::{DomainError, PipelineStage};
use orchestration_http_server::create_app_routes;
use orchestration_infra_alignment::{connect_alignment_client, AlignmentEnrichStage};
use orchestration_infra_asr::{connect_asr_client, AsrTranscribeStage};
use orchestration_infra_audio::{connect_audio_client, AudioTransformStage};
use rustycog_command::GenericCommandService;
use rustycog_config::ServerConfig;
use rustycog_http::{AppState, UserIdExtractor};

pub async fn build_and_run(config: AppConfig, server_config: ServerConfig) -> Result<(), Error> {
    let app = Application::new(config).await?;
    app.run(server_config).await
}

pub struct Application {
    pub config: AppConfig,
    pub state: AppState,
}

impl Application {
    pub async fn new(config: AppConfig) -> Result<Self, Error> {
        let selected = config.service.pipeline.selected.clone();
        let definition = config
            .service
            .pipeline
            .definitions
            .get(&selected)
            .ok_or_else(|| anyhow!("missing pipeline definition `{selected}`"))?;
        let pipeline_definition = build_pipeline_definition(definition);

        let audio_client = connect_with_retry("audio", || async {
            connect_audio_client(
                &grpc_endpoint_uri(&config.service.audio),
                connect_timeout(&config.service.audio),
                config.service.audio.max_decoding_message_bytes,
                config.service.audio.max_encoding_message_bytes,
            )
            .await
        })
        .await?;
        let asr_client = connect_with_retry("asr", || async {
            connect_asr_client(
                &grpc_endpoint_uri(&config.service.asr),
                connect_timeout(&config.service.asr),
                config.service.asr.max_decoding_message_bytes,
                config.service.asr.max_encoding_message_bytes,
            )
            .await
        })
        .await?;
        let alignment_client = connect_with_retry("alignment", || async {
            connect_alignment_client(
                &grpc_endpoint_uri(&config.service.alignment),
                connect_timeout(&config.service.alignment),
                config.service.alignment.max_decoding_message_bytes,
                config.service.alignment.max_encoding_message_bytes,
            )
            .await
        })
        .await?;

        let audio_stage: Arc<dyn PipelineStage> = Arc::new(AudioTransformStage::new(
            audio_client,
            request_timeout(&config.service.audio),
            None,
        ));
        let asr_stage: Arc<dyn PipelineStage> = Arc::new(AsrTranscribeStage::new(
            asr_client,
            request_timeout(&config.service.asr),
        ));
        let alignment_stage: Arc<dyn PipelineStage> = Arc::new(AlignmentEnrichStage::new(
            alignment_client,
            request_timeout(&config.service.alignment),
        ));
        let loader = GrpcPipelineStepLoader {
            audio_transform: audio_stage,
            asr_transcribe: asr_stage,
            alignment_enrich: alignment_stage,
        };
        let pipeline = PipelineEngine::from_definition(&pipeline_definition, &loader)?;

        let usecase: Arc<dyn AsrUseCase> = Arc::new(AsrUseCaseImpl::new(pipeline, 16_000));
        let registry = AsrCommandRegistryFactory::create_registry(usecase);
        let command_service = Arc::new(GenericCommandService::new(Arc::new(registry)));
        let state = AppState::new(command_service, UserIdExtractor::new());

        Ok(Self { config, state })
    }

    pub async fn run(self, server_config: ServerConfig) -> Result<(), Error> {
        create_app_routes(self.state, server_config)
            .await
            .map_err(|err| anyhow!("orchestration http server failed: {err}"))
    }
}

struct GrpcPipelineStepLoader {
    audio_transform: Arc<dyn PipelineStage>,
    asr_transcribe: Arc<dyn PipelineStage>,
    alignment_enrich: Arc<dyn PipelineStage>,
}

impl PipelineStepLoader for GrpcPipelineStepLoader {
    fn load_step(&self, step: &PipelineStepSpec) -> Result<Arc<dyn PipelineStage>, DomainError> {
        match step.name.as_str() {
            "audio_transform" => Ok(self.audio_transform.clone()),
            "asr_transcribe" => Ok(self.asr_transcribe.clone()),
            "alignment_enrich" => Ok(self.alignment_enrich.clone()),
            _ => Err(DomainError::internal_error(&format!(
                "unknown pipeline step `{}`",
                step.name
            ))),
        }
    }
}

fn build_pipeline_definition(definition: &PipelineDefinitionConfig) -> PipelineDefinition {
    PipelineDefinition {
        pre: definition
            .pre
            .iter()
            .map(|step| PipelineStepSpec::new(step.name()))
            .collect(),
        transcription: PipelineStepSpec::new(definition.transcription.name()),
        post: definition
            .post
            .iter()
            .map(|step| PipelineStepSpec::new(step.name()))
            .collect(),
    }
}

fn grpc_endpoint_uri(config: &GrpcEndpointConfig) -> String {
    let scheme = if config.tls_enabled { "https" } else { "http" };
    format!("{scheme}://{}:{}", config.host, config.port)
}

fn connect_timeout(config: &GrpcEndpointConfig) -> Duration {
    Duration::from_millis(config.connect_timeout_ms.max(1))
}

fn request_timeout(config: &GrpcEndpointConfig) -> Duration {
    Duration::from_millis(config.request_timeout_ms.max(1))
}

async fn connect_with_retry<C, F, Fut>(service: &str, mut connect_fn: F) -> Result<C, Error>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<C, DomainError>>,
{
    let mut last_error = None;
    for _ in 0..20 {
        match connect_fn().await {
            Ok(client) => return Ok(client),
            Err(err) => {
                last_error = Some(err);
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
    Err(anyhow!(
        "failed to connect to {service} service after retries: {}",
        last_error
            .map(|err| err.to_string())
            .unwrap_or_else(|| "unknown connection error".to_string())
    ))
}

#[cfg(test)]
mod tests {
    use std::net::{SocketAddr, TcpListener};

    use alignment_grpc_server::pb as alignment_pb;
    use asr_grpc_server::pb as asr_pb;
    use audio_grpc_server::pb as audio_pb;
    use orchestration_application::{TranscribeAudioCommand, TranscribeAudioRequest};
    use orchestration_configuration::{PipelineDefinitionConfig, PipelineStepRef};
    use rustycog_command::CommandContext;
    use tonic::{transport::Server, Request, Response, Status};

    use super::*;

    struct MockAudioService;

    #[tonic::async_trait]
    impl audio_pb::audio_service_server::AudioService for MockAudioService {
        async fn transform_audio(
            &self,
            request: Request<audio_pb::TransformAudioRequest>,
        ) -> Result<Response<audio_pb::TransformAudioResponse>, Status> {
            let request = request.into_inner();
            Ok(Response::new(audio_pb::TransformAudioResponse {
                session_id: request
                    .session_id
                    .unwrap_or_else(|| "generated-audio-session".to_string()),
                samples: request.samples,
                sample_rate_hz: request.target_sample_rate_hz.unwrap_or(
                    request.sample_rate_hz.unwrap_or(16_000),
                ),
                metadata: Some(audio_pb::TransformMetadata {
                    clamped: false,
                    resampled: request.target_sample_rate_hz.is_some(),
                    input_sample_count: 3,
                    output_sample_count: 3,
                    source_sample_rate_hz: request.sample_rate_hz.unwrap_or(16_000),
                    target_sample_rate_hz: request
                        .target_sample_rate_hz
                        .unwrap_or(request.sample_rate_hz.unwrap_or(16_000)),
                }),
            }))
        }
    }

    struct MockAsrService;

    #[tonic::async_trait]
    impl asr_pb::asr_service_server::AsrService for MockAsrService {
        async fn transcribe(
            &self,
            request: Request<asr_pb::TranscribeAudioRequest>,
        ) -> Result<Response<asr_pb::TranscribeAudioResponse>, Status> {
            let request = request.into_inner();
            Ok(Response::new(asr_pb::TranscribeAudioResponse {
                session_id: request
                    .session_id
                    .unwrap_or_else(|| "generated-asr-session".to_string()),
                transcript: Some(asr_pb::Transcript {
                    language: Some(asr_pb::LanguageTag {
                        code: 2,
                        other: None,
                    }),
                    segments: vec![asr_pb::TranscriptSegment {
                        text: "hello world".to_string(),
                        start_ms: 0,
                        end_ms: 250,
                        tokens: vec![asr_pb::TranscriptToken {
                            text: "hello".to_string(),
                            start_ms: 0,
                            end_ms: 120,
                            confidence: 0.97,
                        }],
                    }],
                }),
                text: "hello world".to_string(),
            }))
        }
    }

    struct MockAlignmentService;

    #[tonic::async_trait]
    impl alignment_pb::alignment_service_server::AlignmentService for MockAlignmentService {
        async fn enrich_transcript(
            &self,
            request: Request<alignment_pb::EnrichTranscriptRequest>,
        ) -> Result<Response<alignment_pb::EnrichTranscriptResponse>, Status> {
            let request = request.into_inner();
            let transcript = request
                .transcript
                .ok_or_else(|| Status::invalid_argument("transcript is required"))?;
            Ok(Response::new(alignment_pb::EnrichTranscriptResponse {
                session_id: request
                    .session_id
                    .unwrap_or_else(|| "generated-alignment-session".to_string()),
                transcript: Some(transcript),
                aligned_words: vec![alignment_pb::WordTiming {
                    word: "hello".to_string(),
                    start_ms: 0,
                    end_ms: 120,
                    confidence: 0.95,
                }],
                text: "hello world".to_string(),
            }))
        }
    }

    struct FakeStage {
        id: &'static str,
    }

    #[async_trait::async_trait]
    impl PipelineStage for FakeStage {
        fn name(&self) -> &'static str {
            self.id
        }

        async fn execute(&self, _context: &mut orchestration_domain::PipelineContext) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[test]
    fn pipeline_definition_preserves_step_order() {
        let definition = PipelineDefinitionConfig {
            pre: vec![PipelineStepRef::Name("audio_transform".to_string())],
            transcription: PipelineStepRef::WithName {
                name: "asr_transcribe".to_string(),
            },
            post: vec![PipelineStepRef::Name("alignment_enrich".to_string())],
        };
        let built = build_pipeline_definition(&definition);
        let ordered = built.ordered_steps();
        assert_eq!(ordered[0].name, "audio_transform");
        assert_eq!(ordered[1].name, "asr_transcribe");
        assert_eq!(ordered[2].name, "alignment_enrich");
    }

    #[test]
    fn loader_maps_remote_step_names() {
        let loader = GrpcPipelineStepLoader {
            audio_transform: Arc::new(FakeStage {
                id: "audio_transform",
            }),
            asr_transcribe: Arc::new(FakeStage {
                id: "asr_transcribe",
            }),
            alignment_enrich: Arc::new(FakeStage {
                id: "alignment_enrich",
            }),
        };

        let audio = loader
            .load_step(&PipelineStepSpec::new("audio_transform"))
            .expect("audio stage should exist");
        let asr = loader
            .load_step(&PipelineStepSpec::new("asr_transcribe"))
            .expect("asr stage should exist");
        let alignment = loader
            .load_step(&PipelineStepSpec::new("alignment_enrich"))
            .expect("alignment stage should exist");

        assert_eq!(audio.name(), "audio_transform");
        assert_eq!(asr.name(), "asr_transcribe");
        assert_eq!(alignment.name(), "alignment_enrich");
        assert!(loader.load_step(&PipelineStepSpec::new("unknown_step")).is_err());
    }

    #[tokio::test]
    async fn command_flow_orchestrates_remote_stages() {
        let audio_port = pick_free_port();
        let asr_port = pick_free_port();
        let alignment_port = pick_free_port();

        let audio_server = tokio::spawn(start_audio_server(audio_port));
        let asr_server = tokio::spawn(start_asr_server(asr_port));
        let alignment_server = tokio::spawn(start_alignment_server(alignment_port));

        tokio::time::sleep(Duration::from_millis(75)).await;

        let mut config = AppConfig::default();
        config.service.audio.host = "127.0.0.1".to_string();
        config.service.audio.port = audio_port;
        config.service.asr.host = "127.0.0.1".to_string();
        config.service.asr.port = asr_port;
        config.service.alignment.host = "127.0.0.1".to_string();
        config.service.alignment.port = alignment_port;
        config.service.pipeline.selected = "integration".to_string();
        config.service.pipeline.definitions.insert(
            "integration".to_string(),
            PipelineDefinitionConfig {
                pre: vec![PipelineStepRef::Name("audio_transform".to_string())],
                transcription: PipelineStepRef::Name("asr_transcribe".to_string()),
                post: vec![PipelineStepRef::Name("alignment_enrich".to_string())],
            },
        );

        let app = Application::new(config).await.expect("app should initialize");
        let response = app
            .state
            .command_service
            .execute(
                TranscribeAudioCommand::new(TranscribeAudioRequest {
                    samples: vec![0.1, 0.2, 0.3],
                    sample_rate_hz: Some(16_000),
                    language_hint: Some("en".to_string()),
                    session_id: Some("integration-session".to_string()),
                }),
                CommandContext::new(),
            )
            .await
            .expect("pipeline command should succeed");

        assert_eq!(response.session_id, "integration-session");
        assert_eq!(response.text, "hello world");
        assert_eq!(response.aligned_words.len(), 1);

        audio_server.abort();
        asr_server.abort();
        alignment_server.abort();
        let _ = audio_server.await;
        let _ = asr_server.await;
        let _ = alignment_server.await;
    }

    async fn start_audio_server(port: u16) {
        let addr: SocketAddr = format!("127.0.0.1:{port}")
            .parse()
            .expect("audio socket address");
        Server::builder()
            .add_service(audio_grpc_server::AudioServiceServer::new(MockAudioService))
            .serve(addr)
            .await
            .expect("audio server should run");
    }

    async fn start_asr_server(port: u16) {
        let addr: SocketAddr = format!("127.0.0.1:{port}")
            .parse()
            .expect("asr socket address");
        Server::builder()
            .add_service(asr_grpc_server::AsrServiceServer::new(MockAsrService))
            .serve(addr)
            .await
            .expect("asr server should run");
    }

    async fn start_alignment_server(port: u16) {
        let addr: SocketAddr = format!("127.0.0.1:{port}")
            .parse()
            .expect("alignment socket address");
        Server::builder()
            .add_service(alignment_grpc_server::AlignmentServiceServer::new(MockAlignmentService))
            .serve(addr)
            .await
            .expect("alignment server should run");
    }

    fn pick_free_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .expect("bind ephemeral port")
            .local_addr()
            .expect("extract local address")
            .port()
    }
}
