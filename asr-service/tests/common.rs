use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use rustycog_config::ServerConfig;
use rustycog_testing::{ServiceTestDescriptor, TestFixture};

use asr_configuration::AppConfig;
use asr_setup::build_and_run;

pub struct AsrTestDescriptor;

#[async_trait]
impl ServiceTestDescriptor<TestFixture> for AsrTestDescriptor {
    type Config = AppConfig;

    async fn build_app(
        &self,
        _config: AppConfig,
        _server_config: ServerConfig,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn run_app(&self, config: AppConfig, server_config: ServerConfig) -> anyhow::Result<()> {
        build_and_run(config, server_config).await
    }

    async fn run_migrations_up(
        &self,
        _connection: &sea_orm::DatabaseConnection,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn run_migrations_down(
        &self,
        _connection: &sea_orm::DatabaseConnection,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn has_db(&self) -> bool {
        false
    }

    fn has_sqs(&self) -> bool {
        false
    }
}

pub async fn setup_test_server() -> Result<(TestFixture, String, Client), Box<dyn std::error::Error>>
{
    let descriptor = Arc::new(AsrTestDescriptor);
    let fixture = TestFixture::new(descriptor.clone()).await?;
    let (server_url, client) =
        rustycog_testing::setup_test_server::<AsrTestDescriptor, TestFixture>(descriptor).await?;
    Ok((fixture, server_url, client))
}
