use anyhow::Error;
use asr_application::{AsrCommandRegistryFactory, AsrUseCase, AsrUseCaseImpl};
use asr_configuration::AppConfig;
use asr_domain::TranscriptionPort;
use asr_grpc_server::serve_grpc;
use asr_infra_asr_whisper::{WhisperAdapterConfig, WhisperTranscriptionAdapter};
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
            "initializing ASR application"
        );

        let transcription: Arc<dyn TranscriptionPort> =
            Arc::new(WhisperTranscriptionAdapter::new(WhisperAdapterConfig {
                model_path: config.service.asr.model_path.clone(),
                language: config.service.asr.default_language.clone(),
                temperature: config.service.asr.temperature,
                threads: config.service.asr.threads,
                dtw_preset: config.service.asr.dtw_preset.clone(),
                dtw_mem_size: normalize_dtw_mem_size(config.service.asr.dtw_mem_size),
            }));
        let usecase: Arc<dyn AsrUseCase> = Arc::new(AsrUseCaseImpl::new(
            transcription,
            config.service.audio.sample_rate_hz,
        ));
        let registry = AsrCommandRegistryFactory::create_registry(usecase);
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
            "starting ASR gRPC server"
        );

        serve_grpc(self.command_service, server_config)
            .await
            .map_err(|err| anyhow::anyhow!("server startup failed: {err}"))
    }
}

fn normalize_dtw_mem_size(raw: usize) -> usize {
    const ONE_MIB: usize = 1024 * 1024;
    if raw < ONE_MIB {
        raw.saturating_mul(ONE_MIB)
    } else {
        raw
    }
}
