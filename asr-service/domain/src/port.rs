use async_trait::async_trait;

use crate::{DomainError, TranscriptionOutput, TranscriptionRequest};

#[async_trait]
pub trait TranscriptionPort: Send + Sync {
    async fn transcribe(
        &self,
        request: TranscriptionRequest,
    ) -> Result<TranscriptionOutput, DomainError>;
}
