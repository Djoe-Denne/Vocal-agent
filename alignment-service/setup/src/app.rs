use anyhow::Error;
use alignment_application::{
    AlignTranscriptUseCase, AlignTranscriptUseCaseImpl, AlignmentCommandRegistryFactory,
};
use alignment_configuration::AppConfig;
use alignment_domain::AlignmentPort;
use alignment_grpc_server::serve_grpc;
use alignment_infra_alignment::{Wav2Vec2AdapterConfig, Wav2Vec2ForcedAligner};
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
            sample_rate_hz = config.alignment.sample_rate_hz,
            model_path = %config.alignment.model_path,
            device = %config.alignment.device,
            "initializing alignment application"
        );

        let adapter_cfg = Wav2Vec2AdapterConfig {
            model_path: config.alignment.model_path.clone(),
            config_path: config.alignment.config_path.clone(),
            vocab_path: config.alignment.vocab_path.clone(),
            device: config.alignment.device.clone(),
        };
        let aligner: Arc<dyn AlignmentPort> = Arc::new(
            Wav2Vec2ForcedAligner::load(&adapter_cfg)
                .map_err(|err| anyhow::anyhow!("wav2vec2 model loading failed: {err}"))?,
        );
        let usecase: Arc<dyn AlignTranscriptUseCase> = Arc::new(AlignTranscriptUseCaseImpl::new(
            aligner,
            config.alignment.sample_rate_hz,
        ));
        let registry = AlignmentCommandRegistryFactory::create_registry(usecase);
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
            "starting alignment gRPC server"
        );

        serve_grpc(self.command_service, server_config)
            .await
            .map_err(|err| anyhow::anyhow!("server startup failed: {err}"))
    }
}
