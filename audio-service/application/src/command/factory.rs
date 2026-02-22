use std::sync::Arc;

use rustycog_command::{CommandRegistry, CommandRegistryBuilder};

use crate::{
    AudioCommandErrorMapper, TransformAudioCommand, TransformAudioCommandHandler,
    TransformAudioUseCase,
};

pub struct AudioCommandRegistryFactory;

impl AudioCommandRegistryFactory {
    pub fn create_registry(usecase: Arc<dyn TransformAudioUseCase>) -> CommandRegistry {
        let handler = Arc::new(TransformAudioCommandHandler::new(usecase));
        let error_mapper = Arc::new(AudioCommandErrorMapper);

        CommandRegistryBuilder::new()
            .register::<TransformAudioCommand, _>(
                "transform_audio".to_string(),
                handler,
                error_mapper,
            )
            .build()
    }
}
