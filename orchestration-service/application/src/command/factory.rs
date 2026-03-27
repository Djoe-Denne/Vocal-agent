use std::sync::Arc;
use std::time::Duration;

use rustycog_command::{CommandRegistry, CommandRegistryBuilder, RegistryConfig, RetryPolicy};

use crate::{
    AsrCommandErrorMapper, AsrUseCase, TranscribeAudioCommand, TranscribeAudioCommandHandler,
};

pub struct AsrCommandRegistryFactory;

impl AsrCommandRegistryFactory {
    pub fn create_registry(asr_usecase: Arc<dyn AsrUseCase>) -> CommandRegistry {
        let handler = Arc::new(TranscribeAudioCommandHandler::new(asr_usecase));
        let error_mapper = Arc::new(AsrCommandErrorMapper);

        let config = RegistryConfig {
            default_timeout: Duration::from_secs(180),
            retry_policy: RetryPolicy {
                max_attempts: 0,
                ..RetryPolicy::default()
            },
            ..RegistryConfig::default()
        };

        CommandRegistryBuilder::with_config(config)
            .register::<TranscribeAudioCommand, _>(
                "transcribe_audio".to_string(),
                handler,
                error_mapper,
            )
            .build()
    }
}
