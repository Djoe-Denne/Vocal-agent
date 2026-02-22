use std::sync::Arc;

use rustycog_command::{CommandRegistry, CommandRegistryBuilder};

use crate::{
    AlignTranscriptUseCase, AlignmentCommandErrorMapper, EnrichTranscriptCommand,
    EnrichTranscriptCommandHandler,
};

pub struct AlignmentCommandRegistryFactory;

impl AlignmentCommandRegistryFactory {
    pub fn create_registry(usecase: Arc<dyn AlignTranscriptUseCase>) -> CommandRegistry {
        let handler = Arc::new(EnrichTranscriptCommandHandler::new(usecase));
        let error_mapper = Arc::new(AlignmentCommandErrorMapper);

        CommandRegistryBuilder::new()
            .register::<EnrichTranscriptCommand, _>(
                "enrich_transcript".to_string(),
                handler,
                error_mapper,
            )
            .build()
    }
}
