use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use alignment_domain::{AlignmentPort, AlignmentRequest, AudioChunk};

use crate::{ApplicationError, EnrichTranscriptRequest, EnrichTranscriptResponse};

#[async_trait]
pub trait AlignTranscriptUseCase: Send + Sync {
    async fn enrich_transcript(
        &self,
        request: EnrichTranscriptRequest,
    ) -> Result<EnrichTranscriptResponse, ApplicationError>;
}

pub struct AlignTranscriptUseCaseImpl {
    aligner: Arc<dyn AlignmentPort>,
    default_sample_rate_hz: u32,
}

impl AlignTranscriptUseCaseImpl {
    pub fn new(aligner: Arc<dyn AlignmentPort>, default_sample_rate_hz: u32) -> Self {
        Self {
            aligner,
            default_sample_rate_hz,
        }
    }
}

#[async_trait]
impl AlignTranscriptUseCase for AlignTranscriptUseCaseImpl {
    async fn enrich_transcript(
        &self,
        request: EnrichTranscriptRequest,
    ) -> Result<EnrichTranscriptResponse, ApplicationError> {
        let sample_rate_hz = request.sample_rate_hz.unwrap_or(self.default_sample_rate_hz);
        let session_id = request
            .session_id
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let transcript = request.transcript;
        let text = transcript
            .segments
            .iter()
            .map(|segment| segment.text.trim())
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");

        tracing::debug!(
            session_id = %session_id,
            sample_count = request.samples.len(),
            sample_rate_hz,
            transcript_segment_count = transcript.segments.len(),
            "starting transcript enrichment"
        );

        let aligned_words = self
            .aligner
            .align(AlignmentRequest {
                audio: AudioChunk {
                    sample_rate_hz,
                    samples: request.samples,
                },
                transcript: transcript.clone(),
            })
            .await?
            .words;

        tracing::debug!(
            session_id = %session_id,
            aligned_word_count = aligned_words.len(),
            "transcript enrichment completed"
        );

        Ok(EnrichTranscriptResponse {
            session_id,
            transcript,
            aligned_words,
            text,
        })
    }
}
