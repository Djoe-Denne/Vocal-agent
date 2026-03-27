use std::sync::Arc;

use async_trait::async_trait;
use rustycog_command::{Command, CommandError, CommandErrorMapper, CommandHandler};
use uuid::Uuid;

use crate::{MatchTempoRequest, MatchTempoResponse, TempoMatchUseCase};

#[derive(Debug, Clone)]
pub struct MatchTempoCommand {
    id: Uuid,
    pub request: MatchTempoRequest,
}

impl MatchTempoCommand {
    pub fn new(request: MatchTempoRequest) -> Self {
        Self {
            id: Uuid::new_v4(),
            request,
        }
    }
}

impl Command for MatchTempoCommand {
    type Result = MatchTempoResponse;

    fn command_type(&self) -> &'static str {
        "match_tempo"
    }

    fn command_id(&self) -> Uuid {
        self.id
    }

    fn validate(&self) -> Result<(), CommandError> {
        if self.request.tts_samples.is_empty() {
            return Err(CommandError::validation(
                "samples_missing",
                "tts_samples must contain at least one frame",
            ));
        }

        if self.request.original_timings.is_empty() {
            return Err(CommandError::validation(
                "original_timings_missing",
                "original_timings must contain at least one word",
            ));
        }

        if self.request.tts_timings.is_empty() {
            return Err(CommandError::validation(
                "tts_timings_missing",
                "tts_timings must contain at least one word",
            ));
        }

        Ok(())
    }
}

pub struct MatchTempoCommandHandler {
    usecase: Arc<dyn TempoMatchUseCase>,
}

impl MatchTempoCommandHandler {
    pub fn new(usecase: Arc<dyn TempoMatchUseCase>) -> Self {
        Self { usecase }
    }
}

#[async_trait]
impl CommandHandler<MatchTempoCommand> for MatchTempoCommandHandler {
    async fn handle(
        &self,
        command: MatchTempoCommand,
    ) -> Result<MatchTempoResponse, CommandError> {
        self.usecase
            .match_tempo(command.request)
            .await
            .map_err(CommandError::from)
    }
}

pub struct TempoCommandErrorMapper;

impl CommandErrorMapper for TempoCommandErrorMapper {
    fn map_error(&self, error: Box<dyn std::error::Error + Send + Sync>) -> CommandError {
        CommandError::infrastructure("tempo_command_error", error.to_string())
    }
}
