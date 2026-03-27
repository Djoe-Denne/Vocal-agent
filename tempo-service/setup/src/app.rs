use std::sync::Arc;

use anyhow::Error;
use tempo_application::{TempoCommandRegistryFactory, TempoMatchUseCase, TempoMatchUseCaseImpl};
use tempo_configuration::AppConfig;
use tempo_domain::TempoMatchPort;
use tempo_grpc_server::serve_grpc;
use tempo_infra::TempoMatchAdapter;
use rustycog_command::GenericCommandService;
use rustycog_config::ServerConfig;

pub async fn build_and_run(config: AppConfig, server_config: ServerConfig) -> Result<(), Error> {
    let app = Application::new(config).await?;
    app.run(server_config).await
}

pub struct Application {
    pub config: AppConfig,
    pub command_service: Arc<GenericCommandService>,
}

impl Application {
    pub async fn new_with_matcher(
        config: AppConfig,
        matcher: Arc<dyn TempoMatchPort>,
    ) -> Result<Self, Error> {
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

    async fn new(config: AppConfig) -> Result<Self, Error> {
        let matcher: Arc<dyn TempoMatchPort> = Arc::new(TempoMatchAdapter::new());
        Self::new_with_matcher(config, matcher).await
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
