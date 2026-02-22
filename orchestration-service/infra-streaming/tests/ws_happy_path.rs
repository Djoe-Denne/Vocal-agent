use std::sync::Arc;

use asr_application::{AsrSessionUseCase, PipelineEngine};
use asr_domain::{
    DomainError, DomainEvent, LanguageTag, PipelineContext, PipelineStage, Transcript,
    TranscriptSegment, WordTiming,
};
use asr_infra_streaming::{build_router, StreamingState};
use async_trait::async_trait;
use axum::serve;
use futures::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::{connect_async, tungstenite::Message};

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
                text: "bonjour world".to_string(),
                start_ms: 0,
                end_ms: 700,
                tokens: Vec::new(),
            }],
        };
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
        context.events.push(DomainEvent::AlignmentUpdate {
            words: vec![WordTiming {
                word: "bonjour".to_string(),
                start_ms: 0,
                end_ms: 350,
                confidence: 0.95,
            }],
        });
        Ok(())
    }
}

#[tokio::test]
async fn websocket_session_emits_transcript_and_alignment() {
    let usecase = Arc::new(AsrSessionUseCase::new(PipelineEngine::new(vec![
        Arc::new(MockAsrStage),
        Arc::new(MockAlignStage),
    ])));
    let app = build_router(StreamingState {
        usecase,
        max_message_bytes: 1024 * 1024,
    });

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let server = tokio::spawn(async move {
        serve(listener, app).await.expect("server run");
    });

    let ws_url = format!("ws://{}/ws", addr);
    let (mut socket, _) = connect_async(ws_url).await.expect("connect");

    socket
        .send(Message::Text(
            r#"{"version":1,"type":"start","payload":{"session_id":"it"}}"#
                .to_string()
                .into(),
        ))
        .await
        .expect("send start");
    socket
        .send(Message::Text(
            r#"{"version":1,"type":"audio_frame","payload":{"pcm_f32":[0.0,0.1,0.2]}}"#
                .to_string()
                .into(),
        ))
        .await
        .expect("send audio");
    socket
        .send(Message::Text(r#"{"version":1,"type":"flush"}"#.to_string().into()))
        .await
        .expect("send flush");

    let mut got_ready = false;
    let mut got_final = false;
    let mut got_align = false;

    for _ in 0..4 {
        let Some(Ok(msg)) = socket.next().await else {
            continue;
        };
        if let Message::Text(raw) = msg {
            if raw.contains("\"ready\"") {
                got_ready = true;
            }
            if raw.contains("\"final_transcript\"") {
                got_final = true;
            }
            if raw.contains("\"alignment_update\"") {
                got_align = true;
            }
        }
        if got_ready && got_final && got_align {
            break;
        }
    }

    assert!(got_ready, "missing ready event");
    assert!(got_final, "missing final transcript event");
    assert!(got_align, "missing alignment update event");

    server.abort();
}
