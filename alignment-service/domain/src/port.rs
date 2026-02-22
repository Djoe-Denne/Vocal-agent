use async_trait::async_trait;

use crate::{AlignmentOutput, AlignmentRequest, DomainError};

#[async_trait]
pub trait AlignmentPort: Send + Sync {
    async fn align(&self, request: AlignmentRequest) -> Result<AlignmentOutput, DomainError>;
}
