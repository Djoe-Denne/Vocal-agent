use async_trait::async_trait;
use serde_json::json;
use uuid::Uuid;

use orchestration_domain::{DomainEvent, LanguageTag, PipelineContext};

use crate::{ApplicationError, PipelineEngine, TranscribeAudioRequest, TranscribeAudioResponse};

#[async_trait]
pub trait AsrUseCase: Send + Sync {
    async fn transcribe(
        &self,
        request: TranscribeAudioRequest,
    ) -> Result<TranscribeAudioResponse, ApplicationError>;
}

pub struct AsrUseCaseImpl {
    pipeline: PipelineEngine,
    sample_rate_hz: u32,
}

impl AsrUseCaseImpl {
    pub fn new(pipeline: PipelineEngine, sample_rate_hz: u32) -> Self {
        Self {
            pipeline,
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
        tracing::debug!(
            sample_count = request.samples.len(),
            sample_rate_hz = request.sample_rate_hz.unwrap_or(self.sample_rate_hz),
            language_hint = request.language_hint.as_deref().unwrap_or("auto"),
            session_id = request.session_id.as_deref().unwrap_or("auto"),
            "starting asr pipeline"
        );

        let input_sample_rate_hz = request.sample_rate_hz.unwrap_or(self.sample_rate_hz);
        let mut context = PipelineContext::new(
            request
                .session_id
                .clone()
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            parse_language_hint(request.language_hint.as_deref())?,
        );
        context.audio.sample_rate_hz = input_sample_rate_hz;
        context.audio.samples = request.samples;
        context.set_extension("audio.request_sample_rate_hz", json!(input_sample_rate_hz));
        self.pipeline.run(&mut context).await?;

        let transcript = context.transcript.clone().ok_or_else(|| {
            ApplicationError::Internal("transcription pipeline returned no transcript".to_string())
        })?;
        let text = transcript
            .segments
            .iter()
            .map(|segment| segment.text.trim())
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");

        let aligned_words = extract_alignment_words(&context);
        let response = TranscribeAudioResponse {
            session_id: context.session_id,
            transcript,
            aligned_words,
            text,
        };

        tracing::debug!(
            segment_count = response.transcript.segments.len(),
            aligned_word_count = response.aligned_words.len(),
            "asr pipeline completed"
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

fn extract_alignment_words(context: &PipelineContext) -> Vec<orchestration_domain::WordTiming> {
    if !context.aligned_words.is_empty() {
        return context.aligned_words.clone();
    }

    for event in &context.events {
        if let DomainEvent::AlignmentUpdate { words } = event {
            return words.clone();
        }
    }
    Vec::new()
}
