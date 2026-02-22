use std::sync::Arc;

use asr_application::{AsrUseCase, AsrUseCaseImpl, TranscribeAudioRequest};
use asr_domain::{
    DomainError, LanguageTag, Transcript, TranscriptSegment, TranscriptionOutput,
    TranscriptionPort, TranscriptionRequest,
};
use async_trait::async_trait;

struct MockTranscriptionPort;

#[async_trait]
impl TranscriptionPort for MockTranscriptionPort {
    async fn transcribe(
        &self,
        request: TranscriptionRequest,
    ) -> Result<TranscriptionOutput, DomainError> {
        let transcript = Transcript {
            language: request.language_hint.unwrap_or(LanguageTag::En),
            segments: vec![TranscriptSegment {
                text: "hello world".to_string(),
                start_ms: 0,
                end_ms: request.audio.samples.len().saturating_mul(10) as u64,
                tokens: Vec::new(),
            }],
        };
        Ok(TranscriptionOutput { transcript })
    }
}

#[tokio::test]
async fn transcribe_command_flow_produces_transcript_text() {
    let usecase: Arc<dyn AsrUseCase> =
        Arc::new(AsrUseCaseImpl::new(Arc::new(MockTranscriptionPort), 16_000));
    let response = usecase
        .transcribe(TranscribeAudioRequest {
            samples: vec![0.1, 0.2, 0.3],
            sample_rate_hz: Some(16_000),
            language_hint: Some("en".to_string()),
            session_id: Some("it-session".to_string()),
        })
        .await
        .expect("transcription succeeds");

    assert_eq!(response.session_id, "it-session");
    assert_eq!(response.transcript.segments.len(), 1);
    assert_eq!(response.text, "hello world");
}
