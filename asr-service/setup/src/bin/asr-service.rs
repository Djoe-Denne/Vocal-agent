use asr_configuration::{load_config, setup_logging};
use asr_setup::build_and_run;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = load_config()?;
    setup_logging(&config);
    let server_config = config.server.clone();
    build_and_run(config, server_config).await?;
    Ok(())
}
