use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use asr_domain::{AudioChunk, LanguageTag, TranscriptionPort, TranscriptionRequest};

use crate::{ApplicationError, TranscribeAudioRequest, TranscribeAudioResponse};

#[async_trait]
pub trait AsrUseCase: Send + Sync {
    async fn transcribe(
        &self,
        request: TranscribeAudioRequest,
    ) -> Result<TranscribeAudioResponse, ApplicationError>;
}

pub struct AsrUseCaseImpl {
    transcription: Arc<dyn TranscriptionPort>,
    sample_rate_hz: u32,
}

impl AsrUseCaseImpl {
    pub fn new(transcription: Arc<dyn TranscriptionPort>, sample_rate_hz: u32) -> Self {
        Self {
            transcription,
            sample_rate_hz,
        }
    }
}

#[async_trait]
impl AsrUseCase for AsrUseCaseImpl {
    async fn transcribe(
        &self,
        request: TranscribeAudioRequest,
    ) -> Result<TranscribeAudioResponse, ApplicationError> {
        let TranscribeAudioRequest {
            samples,
            sample_rate_hz,
            language_hint,
            session_id,
        } = request;
        tracing::debug!(
            sample_count = samples.len(),
            sample_rate_hz = sample_rate_hz.unwrap_or(self.sample_rate_hz),
            language_hint = language_hint.as_deref().unwrap_or("auto"),
            session_id = session_id.as_deref().unwrap_or("auto"),
            "starting asr transcription"
        );

        let input_sample_rate_hz = sample_rate_hz.unwrap_or(self.sample_rate_hz);
        let session_id = session_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let transcript = self
            .transcription
            .transcribe(TranscriptionRequest {
                language_hint: parse_language_hint(language_hint.as_deref())?,
                audio: AudioChunk {
                    sample_rate_hz: input_sample_rate_hz,
                    samples,
                },
            })
            .await?
            .transcript;
        let text = transcript
            .segments
            .iter()
            .map(|segment| segment.text.trim())
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");

        let response = TranscribeAudioResponse {
            session_id,
            transcript,
            text,
        };

        tracing::debug!(
            segment_count = response.transcript.segments.len(),
            "asr transcription completed"
        );

        Ok(response)
    }
}

fn parse_language_hint(value: Option<&str>) -> Result<Option<LanguageTag>, ApplicationError> {
    let Some(language) = value else {
        return Ok(None);
    };

    let parsed = match language.to_ascii_lowercase().as_str() {
        "fr" => LanguageTag::Fr,
        "en" => LanguageTag::En,
        "auto" => LanguageTag::Auto,
        other if !other.is_empty() => LanguageTag::Other(other.to_string()),
        _ => {
            return Err(ApplicationError::Validation(
                "language_hint cannot be empty".to_string(),
            ));
        }
    };
    Ok(Some(parsed))
}
