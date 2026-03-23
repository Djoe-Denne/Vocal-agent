use async_trait::async_trait;

use crate::{DomainError, TempoMatchOutput, TempoMatchRequest};

#[async_trait]
pub trait TempoMatchPort: Send + Sync {
    async fn match_tempo(
        &self,
        request: TempoMatchRequest,
    ) -> Result<TempoMatchOutput, DomainError>;
}
