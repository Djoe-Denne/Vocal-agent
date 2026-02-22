use async_trait::async_trait;

use crate::{AudioTransformRequest, AudioTransformResult, DomainError};

#[async_trait]
pub trait AudioTransformPort: Send + Sync {
    async fn transform(
        &self,
        request: AudioTransformRequest,
    ) -> Result<AudioTransformResult, DomainError>;
}
