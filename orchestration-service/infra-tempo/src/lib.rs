use std::time::Duration;

use async_trait::async_trait;
use orchestration_domain::{DomainError, PipelineContext, PipelineStage};

pub struct TempoMatchStage {
    _request_timeout: Duration,
}

impl TempoMatchStage {
    pub fn new(request_timeout: Duration) -> Self {
        Self {
            _request_timeout: request_timeout,
        }
    }
}

#[async_trait]
impl PipelineStage for TempoMatchStage {
    fn name(&self) -> &'static str {
        "tempo_match"
    }

    async fn execute(&self, _context: &mut PipelineContext) -> Result<(), DomainError> {
        Err(DomainError::internal_error(
            "tempo_match stage has no implementation configured",
        ))
    }
}
