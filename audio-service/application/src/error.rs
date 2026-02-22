use rustycog_command::CommandError;
use audio_domain::DomainError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("Domain error: {0}")]
    Domain(#[from] DomainError),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<ApplicationError> for CommandError {
    fn from(error: ApplicationError) -> Self {
        match error {
            ApplicationError::Domain(err) => {
                CommandError::business("domain_error", err.to_string())
            }
            ApplicationError::Validation(message) => {
                CommandError::validation("validation_error", message)
            }
            ApplicationError::Internal(message) => {
                CommandError::infrastructure("internal_error", message)
            }
        }
    }
}
