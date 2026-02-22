use std::sync::Arc;

use rustycog_command::{CommandRegistry, CommandRegistryBuilder};

use crate::{
    AsrCommandErrorMapper, AsrUseCase, TranscribeAudioCommand, TranscribeAudioCommandHandler,
};

pub struct AsrCommandRegistryFactory;

impl AsrCommandRegistryFactory {
    pub fn create_registry(asr_usecase: Arc<dyn AsrUseCase>) -> CommandRegistry {
        let handler = Arc::new(TranscribeAudioCommandHandler::new(asr_usecase));
        let error_mapper = Arc::new(AsrCommandErrorMapper);

        CommandRegistryBuilder::new()
            .register::<TranscribeAudioCommand, _>(
                "transcribe_audio".to_string(),
                handler,
                error_mapper,
            )
            .build()
    }
}
