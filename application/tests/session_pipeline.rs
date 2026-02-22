use std::sync::Arc;

use asr_application::{AsrUseCase, AsrUseCaseImpl, PipelineEngine, TranscribeAudioRequest};
use asr_domain::{
    DomainError, DomainEvent, LanguageTag, PipelineContext, PipelineStage, Transcript,
    TranscriptSegment, WordTiming,
};
use async_trait::async_trait;

struct MockAsrStage;
struct MockAlignStage;

#[async_trait]
impl PipelineStage for MockAsrStage {
    fn name(&self) -> &'static str {
        "mock-asr"
    }

    async fn execute(&self, context: &mut PipelineContext) -> Result<(), DomainError> {
        let transcript = Transcript {
            language: LanguageTag::En,
            segments: vec![TranscriptSegment {
                text: "hello world".to_string(),
                start_ms: 0,
                end_ms: 500,
                tokens: Vec::new(),
            }],
        };
        context.transcript = Some(transcript.clone());
        context
            .events
            .push(DomainEvent::FinalTranscript { transcript });
        Ok(())
    }
}

#[async_trait]
impl PipelineStage for MockAlignStage {
    fn name(&self) -> &'static str {
        "mock-align"
    }

    async fn execute(&self, context: &mut PipelineContext) -> Result<(), DomainError> {
        let words = vec![WordTiming {
            word: "hello".to_string(),
            start_ms: 0,
            end_ms: 250,
            confidence: 0.9,
        }];
        context.aligned_words = words.clone();
        context.events.push(DomainEvent::AlignmentUpdate { words });
        Ok(())
    }
}

#[tokio::test]
async fn transcribe_command_flow_produces_transcript_and_alignment() {
    let pipeline = PipelineEngine::new(vec![Arc::new(MockAsrStage), Arc::new(MockAlignStage)]);
    let usecase: Arc<dyn AsrUseCase> = Arc::new(AsrUseCaseImpl::new(pipeline, 16_000));
    let response = usecase
        .transcribe(TranscribeAudioRequest {
            samples: vec![0.1, 0.2, 0.3],
            sample_rate_hz: Some(16_000),
            language_hint: Some("en".to_string()),
            session_id: Some("it-session".to_string()),
        })
        .await
        .expect("pipeline succeeds");

    assert_eq!(response.session_id, "it-session");
    assert!(!response.transcript.segments.is_empty());
    assert!(!response.aligned_words.is_empty());
    assert_eq!(response.text, "hello world");
}
