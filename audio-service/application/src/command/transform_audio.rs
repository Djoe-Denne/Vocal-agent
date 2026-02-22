use std::sync::Arc;

use async_trait::async_trait;
use rustycog_command::{Command, CommandError, CommandErrorMapper, CommandHandler};
use uuid::Uuid;

use crate::{TransformAudioRequest, TransformAudioResponse, TransformAudioUseCase};

#[derive(Debug, Clone)]
pub struct TransformAudioCommand {
    id: Uuid,
    pub request: TransformAudioRequest,
}

impl TransformAudioCommand {
    pub fn new(request: TransformAudioRequest) -> Self {
        Self {
            id: Uuid::new_v4(),
            request,
        }
    }
}

impl Command for TransformAudioCommand {
    type Result = TransformAudioResponse;

    fn command_type(&self) -> &'static str {
        "transform_audio"
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

pub struct TransformAudioCommandHandler {
    usecase: Arc<dyn TransformAudioUseCase>,
}

impl TransformAudioCommandHandler {
    pub fn new(usecase: Arc<dyn TransformAudioUseCase>) -> Self {
        Self { usecase }
    }
}

#[async_trait]
impl CommandHandler<TransformAudioCommand> for TransformAudioCommandHandler {
    async fn handle(
        &self,
        command: TransformAudioCommand,
    ) -> Result<TransformAudioResponse, CommandError> {
        let request = command.request;
        self.usecase
            .transform_audio(request)
            .await
            .map_err(CommandError::from)
    }
}

pub struct AudioCommandErrorMapper;

impl CommandErrorMapper for AudioCommandErrorMapper {
    fn map_error(&self, error: Box<dyn std::error::Error + Send + Sync>) -> CommandError {
        CommandError::infrastructure("audio_command_error", error.to_string())
    }
}
