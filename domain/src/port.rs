use async_trait::async_trait;

use crate::{
    AlignmentOutput, AlignmentRequest, DomainError, PipelineContext, TranscriptionOutput,
    TranscriptionRequest,
};

#[async_trait]
pub trait PipelineStage: Send + Sync {
    fn name(&self) -> &'static str;
    async fn execute(&self, context: &mut PipelineContext) -> Result<(), DomainError>;
}

#[async_trait]
pub trait TranscriptionPort: Send + Sync {
    async fn transcribe(
        &self,
        request: TranscriptionRequest,
    ) -> Result<TranscriptionOutput, DomainError>;
}

#[async_trait]
pub trait AlignmentPort: Send + Sync {
    async fn align(&self, request: AlignmentRequest) -> Result<AlignmentOutput, DomainError>;
}
