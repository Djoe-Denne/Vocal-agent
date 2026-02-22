use anyhow::Error;
use audio_application::{
    AudioCommandRegistryFactory, TransformAudioUseCase, TransformAudioUseCaseImpl,
};
use audio_configuration::AppConfig;
use audio_domain::AudioTransformPort;
use audio_grpc_server::serve_grpc;
use audio_infra::AudioTransformerAdapter;
use rustycog_command::GenericCommandService;
use rustycog_config::ServerConfig;
use std::sync::Arc;

pub async fn build_and_run(config: AppConfig, server_config: ServerConfig) -> Result<(), Error> {
    let app = Application::new(config).await?;
    app.run(server_config).await
}

pub struct Application {
    pub config: AppConfig,
    pub command_service: Arc<GenericCommandService>,
}

impl Application {
    pub async fn new(config: AppConfig) -> Result<Self, Error> {
        tracing::info!(
            default_sample_rate_hz = config.transformations.sample_rate_hz,
            default_chunk_ms = config.transformations.chunk_ms,
            "initializing audio transformation application"
        );

        let transformer: Arc<dyn AudioTransformPort> = Arc::new(AudioTransformerAdapter::new());
        let usecase: Arc<dyn TransformAudioUseCase> = Arc::new(TransformAudioUseCaseImpl::new(
            transformer,
            config.transformations.sample_rate_hz,
        ));
        let registry = AudioCommandRegistryFactory::create_registry(usecase);
        let command_service = Arc::new(GenericCommandService::new(Arc::new(registry)));

        Ok(Self {
            config,
            command_service,
        })
    }

    pub async fn run(self, server_config: ServerConfig) -> Result<(), Error> {
        tracing::info!(
            host = %server_config.host,
            port = server_config.port,
            "starting audio gRPC server"
        );

        serve_grpc(self.command_service, server_config)
            .await
            .map_err(|err| anyhow::anyhow!("server startup failed: {err}"))
    }
}
