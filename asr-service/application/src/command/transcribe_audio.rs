use std::sync::Arc;

use async_trait::async_trait;
use rustycog_command::{Command, CommandError, CommandErrorMapper, CommandHandler};
use uuid::Uuid;

use crate::{AsrUseCase, TranscribeAudioRequest, TranscribeAudioResponse};

#[derive(Debug, Clone)]
pub struct TranscribeAudioCommand {
    id: Uuid,
    pub request: TranscribeAudioRequest,
}

impl TranscribeAudioCommand {
    pub fn new(request: TranscribeAudioRequest) -> Self {
        Self {
            id: Uuid::new_v4(),
            request,
        }
    }
}

impl Command for TranscribeAudioCommand {
    type Result = TranscribeAudioResponse;

    fn command_type(&self) -> &'static str {
        "transcribe_audio"
    }

    fn command_id(&self) -> Uuid {
        self.id
    }

    fn validate(&self) -> Result<(), CommandError> {
        if self.request.samples.is_empty() {
            return Err(CommandError::validation(
                "samples_missing",
                "samples must contain at least one frame",
            ));
        }
        Ok(())
    }
}

pub struct TranscribeAudioCommandHandler {
    usecase: Arc<dyn AsrUseCase>,
}

impl TranscribeAudioCommandHandler {
    pub fn new(usecase: Arc<dyn AsrUseCase>) -> Self {
        Self { usecase }
    }
}

#[async_trait]
impl CommandHandler<TranscribeAudioCommand> for TranscribeAudioCommandHandler {
    async fn handle(
        &self,
        command: TranscribeAudioCommand,
    ) -> Result<TranscribeAudioResponse, CommandError> {
        let request = command.request;
        self.usecase
            .transcribe(request)
            .await
            .map_err(CommandError::from)
    }
}

pub struct AsrCommandErrorMapper;

impl CommandErrorMapper for AsrCommandErrorMapper {
    fn map_error(&self, error: Box<dyn std::error::Error + Send + Sync>) -> CommandError {
        CommandError::infrastructure("asr_command_error", error.to_string())
    }
}
