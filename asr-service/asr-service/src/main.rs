use anyhow::Result;
use asr_configuration::{load_config, setup_logging};
use asr_setup::Application;

#[tokio::main]
async fn main() -> Result<()> {
    let config = load_config()?;
    setup_logging(&config);
    let server_config = config.server.clone();
    let app = Application::new(config).await?;
    app.run(server_config).await?;
    Ok(())
}
