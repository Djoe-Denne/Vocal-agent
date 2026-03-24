use std::sync::Arc;

use anyhow::Error;
use tempo_application::{TempoCommandRegistryFactory, TempoMatchUseCase, TempoMatchUseCaseImpl};
use tempo_configuration::AppConfig;
use tempo_domain::TempoMatchPort;
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

    async fn new(_config: AppConfig) -> Result<Self, Error> {
        anyhow::bail!("no TempoMatchPort implementation configured — use Application::new_with_matcher")
    }

    pub async fn run(self, _server_config: ServerConfig) -> Result<(), Error> {
        anyhow::bail!("no transport configured")
    }
}
