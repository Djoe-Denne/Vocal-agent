use std::sync::Arc;

use async_trait::async_trait;
use rustycog_command::{Command, CommandError, CommandErrorMapper, CommandHandler};
use uuid::Uuid;

use crate::{AlignTranscriptUseCase, EnrichTranscriptRequest, EnrichTranscriptResponse};

#[derive(Debug, Clone)]
pub struct EnrichTranscriptCommand {
    id: Uuid,
    pub request: EnrichTranscriptRequest,
}

impl EnrichTranscriptCommand {
    pub fn new(request: EnrichTranscriptRequest) -> Self {
        Self {
            id: Uuid::new_v4(),
            request,
        }
    }
}

impl Command for EnrichTranscriptCommand {
    type Result = EnrichTranscriptResponse;

    fn command_type(&self) -> &'static str {
        "enrich_transcript"
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

        if self.request.transcript.segments.is_empty() {
            return Err(CommandError::validation(
                "transcript_missing",
                "transcript must contain at least one segment",
            ));
        }

        Ok(())
    }
}

pub struct EnrichTranscriptCommandHandler {
    usecase: Arc<dyn AlignTranscriptUseCase>,
}

impl EnrichTranscriptCommandHandler {
    pub fn new(usecase: Arc<dyn AlignTranscriptUseCase>) -> Self {
        Self { usecase }
    }
}

#[async_trait]
impl CommandHandler<EnrichTranscriptCommand> for EnrichTranscriptCommandHandler {
    async fn handle(
        &self,
        command: EnrichTranscriptCommand,
    ) -> Result<EnrichTranscriptResponse, CommandError> {
        self.usecase
            .enrich_transcript(command.request)
            .await
            .map_err(CommandError::from)
    }
}

pub struct AlignmentCommandErrorMapper;

impl CommandErrorMapper for AlignmentCommandErrorMapper {
    fn map_error(&self, error: Box<dyn std::error::Error + Send + Sync>) -> CommandError {
        CommandError::infrastructure("alignment_command_error", error.to_string())
    }
}
