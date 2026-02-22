use std::sync::Arc;

use alignment_application::{
    AlignTranscriptUseCase, AlignTranscriptUseCaseImpl, EnrichTranscriptCommand,
    EnrichTranscriptCommandHandler, EnrichTranscriptRequest,
};
use alignment_domain::{
    AlignmentOutput, AlignmentPort, AlignmentRequest, DomainError, LanguageTag, Transcript,
    TranscriptSegment, WordTiming,
};
use async_trait::async_trait;
use rustycog_command::CommandHandler;

struct MockAlignmentPort;

#[async_trait]
impl AlignmentPort for MockAlignmentPort {
    async fn align(&self, _request: AlignmentRequest) -> Result<AlignmentOutput, DomainError> {
        Ok(AlignmentOutput {
            words: vec![WordTiming {
                word: "hello".to_string(),
                start_ms: 0,
                end_ms: 250,
                confidence: 0.9,
            }],
        })
    }
}

#[tokio::test]
async fn enrich_command_flow_returns_enriched_transcript() {
    let aligner: Arc<dyn AlignmentPort> = Arc::new(MockAlignmentPort);
    let usecase: Arc<dyn AlignTranscriptUseCase> = Arc::new(AlignTranscriptUseCaseImpl::new(
        aligner,
        16_000,
    ));
    let handler = EnrichTranscriptCommandHandler::new(usecase);

    let response = handler
        .handle(EnrichTranscriptCommand::new(EnrichTranscriptRequest {
            samples: vec![0.1, 0.2, 0.3],
            sample_rate_hz: Some(16_000),
            transcript: Transcript {
                language: LanguageTag::En,
                segments: vec![TranscriptSegment {
                    text: "hello world".to_string(),
                    start_ms: 0,
                    end_ms: 500,
                    tokens: Vec::new(),
                }],
            },
            session_id: Some("it-session".to_string()),
        }))
        .await
        .expect("command succeeds");

    assert_eq!(response.session_id, "it-session");
    assert_eq!(response.text, "hello world");
    assert!(!response.aligned_words.is_empty());
}
