use anyhow::Error;
use asr_application::{AsrCommandRegistryFactory, AsrUseCase, AsrUseCaseImpl, PipelineEngine};
use asr_configuration::AppConfig;
use asr_http_server::create_app_routes;
use rustycog_command::GenericCommandService;
use rustycog_config::ServerConfig;
use rustycog_http::{AppState, UserIdExtractor};
use std::sync::Arc;

use crate::pipeline_loader::PipelinePluginLoader;

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
        #[cfg(feature = "whisper-runtime")]
        tracing::info!("whisper runtime feature enabled");
        #[cfg(not(feature = "whisper-runtime"))]
        tracing::warn!(
            "service compiled without `whisper-runtime`; transcription will return fallback text"
        );
        #[cfg(feature = "whisper-cuda")]
        tracing::info!("whisper backend: CUDA");
        #[cfg(feature = "whisper-vulkan")]
        tracing::info!("whisper backend: Vulkan");
        #[cfg(all(
            feature = "whisper-runtime",
            not(feature = "whisper-cuda"),
            not(feature = "whisper-vulkan")
        ))]
        tracing::info!("whisper backend: CPU");

        tracing::info!(
            sample_rate_hz = config.service.audio.sample_rate_hz,
            model_path = %config.service.asr.model_path,
            alignment_enabled = config.service.alignment.enabled,
            pipeline_selected = config
                .service
                .pipeline
                .as_ref()
                .map(|pipeline| pipeline.selected.as_str())
                .unwrap_or("legacy-default"),
            "initializing ASR application"
        );

        let pipeline_loader = PipelinePluginLoader::new(config.clone());
        let pipeline: PipelineEngine = pipeline_loader.build_engine()?;
        let usecase: Arc<dyn AsrUseCase> = Arc::new(AsrUseCaseImpl::new(
            pipeline,
            config.service.audio.sample_rate_hz,
        ));
        let registry = AsrCommandRegistryFactory::create_registry(usecase);
        let command_service = Arc::new(GenericCommandService::new(Arc::new(registry)));
        let state = AppState::new(command_service, UserIdExtractor::new());

        Ok(Self { config, state })
    }

    pub async fn run(self, server_config: ServerConfig) -> Result<(), Error> {
        tracing::info!(
            host = %server_config.host,
            port = server_config.port,
            "starting ASR HTTP routes"
        );

        create_app_routes(self.state, server_config)
            .await
            .map_err(|err| anyhow::anyhow!("server startup failed: {err}"))
    }
}
