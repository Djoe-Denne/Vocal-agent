use anyhow::Error;
use tempo_application::{TempoCommandRegistryFactory, TempoMatchUseCase, TempoMatchUseCaseImpl};
use tempo_configuration::AppConfig;
use tempo_domain::TempoMatchPort;
use tempo_grpc_server::serve_grpc;
use tempo_infra_tempo::{WsolaConfig, WsolaTempoMatcher};
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
            sample_rate_hz = config.tempo.sample_rate_hz,
            wsola_window_ms = config.tempo.wsola_window_ms,
            "initializing tempo application"
        );

        let wsola_config = WsolaConfig {
            sample_rate_hz: config.tempo.sample_rate_hz,
            window_ms: config.tempo.wsola_window_ms,
            overlap_ratio: config.tempo.wsola_overlap_ratio,
            crossfade_ms: config.tempo.crossfade_ms,
            stretch_tolerance: config.tempo.stretch_tolerance,
            max_stretch_ratio: config.tempo.max_stretch_ratio,
            min_stretch_ratio: config.tempo.min_stretch_ratio,
            silence_threshold_db: config.tempo.silence_threshold_db,
        };

        let matcher: Arc<dyn TempoMatchPort> = Arc::new(WsolaTempoMatcher::new(wsola_config));
        let usecase: Arc<dyn TempoMatchUseCase> = Arc::new(TempoMatchUseCaseImpl::new(
            matcher,
            config.tempo.sample_rate_hz,
        ));
        let registry = TempoCommandRegistryFactory::create_registry(usecase);
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
            "starting tempo gRPC server"
        );

        serve_grpc(self.command_service, server_config)
            .await
            .map_err(|err| anyhow::anyhow!("server startup failed: {err}"))
    }
}
