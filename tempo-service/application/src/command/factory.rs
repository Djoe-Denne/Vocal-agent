use std::sync::Arc;

use rustycog_command::{CommandRegistry, CommandRegistryBuilder};

use crate::{
    MatchTempoCommand, MatchTempoCommandHandler, TempoCommandErrorMapper, TempoMatchUseCase,
};

pub struct TempoCommandRegistryFactory;

impl TempoCommandRegistryFactory {
    pub fn create_registry(usecase: Arc<dyn TempoMatchUseCase>) -> CommandRegistry {
        let handler = Arc::new(MatchTempoCommandHandler::new(usecase));
        let error_mapper = Arc::new(TempoCommandErrorMapper);

        CommandRegistryBuilder::new()
            .register::<MatchTempoCommand, _>("match_tempo".to_string(), handler, error_mapper)
            .build()
    }
}
