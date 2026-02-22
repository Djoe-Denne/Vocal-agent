use std::sync::Arc;

use asr_application::AsrSessionUseCase;
use asr_domain::{DomainError, DomainEvent, PipelineContext};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
    routing::get,
    Router,
};
use futures::StreamExt;
use tokio::net::TcpListener;
use tracing::{error, info};
use uuid::Uuid;

pub mod protocol;

use protocol::{ClientEnvelope, ClientMessage, ServerEnvelope, ServerMessage, PROTOCOL_VERSION};

#[derive(Clone)]
pub struct StreamingState {
    pub usecase: Arc<AsrSessionUseCase>,
    pub max_message_bytes: usize,
}

pub fn build_router(state: StreamingState) -> Router {
    Router::new().route("/ws", get(ws_handler)).with_state(state)
}

pub async fn run_server(router: Router, bind_addr: &str) -> Result<(), DomainError> {
    let listener = TcpListener::bind(bind_addr)
        .await
        .map_err(|err| DomainError::Streaming(format!("bind failed: {err}")))?;
    info!("websocket server listening on {}", bind_addr);
    axum::serve(listener, router)
        .await
        .map_err(|err| DomainError::Streaming(format!("server error: {err}")))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<StreamingState>,
) -> Response {
    ws.max_message_size(state.max_message_bytes)
        .on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: StreamingState) {
    let mut context: Option<PipelineContext> = None;
    while let Some(msg_result) = socket.next().await {
        match msg_result {
            Ok(Message::Text(raw)) => {
                if let Err(err) = process_text_message(&mut socket, &state, &mut context, raw.as_str()).await {
                    error!("session error: {}", err);
                    let _ = send_message(
                        &mut socket,
                        ServerMessage::Error {
                            message: err.to_string(),
                        },
                    )
                    .await;
                    return;
                }
            }
            Ok(Message::Binary(_)) => {
                let _ = send_message(
                    &mut socket,
                    ServerMessage::Error {
                        message: "binary frames are not supported; use JSON audio_frame".to_string(),
                    },
                )
                .await;
            }
            Ok(Message::Close(_)) => return,
            Ok(Message::Ping(_)) => {
                let _ = send_message(&mut socket, ServerMessage::Pong).await;
            }
            Ok(_) => {}
            Err(err) => {
                error!("websocket transport error: {}", err);
                return;
            }
        }
    }
}

async fn process_text_message(
    socket: &mut WebSocket,
    state: &StreamingState,
    context: &mut Option<PipelineContext>,
    raw: &str,
) -> Result<(), DomainError> {
    let envelope: ClientEnvelope = serde_json::from_str(raw)
        .map_err(|err| DomainError::Streaming(format!("invalid message: {err}")))?;
    if envelope.version != PROTOCOL_VERSION {
        return Err(DomainError::Streaming(format!(
            "unsupported protocol version {}, expected {}",
            envelope.version, PROTOCOL_VERSION
        )));
    }

    match envelope.message {
        ClientMessage::Start {
            session_id,
            language_hint,
        } => {
            let sid = session_id.unwrap_or_else(|| Uuid::new_v4().to_string());
            *context = Some(PipelineContext::new(sid.clone(), language_hint));
            send_message(socket, ServerMessage::Ready { session_id: sid }).await?;
        }
        ClientMessage::AudioFrame { pcm_f32 } => {
            let ctx = context
                .as_mut()
                .ok_or_else(|| DomainError::Streaming("start must be sent first".to_string()))?;
            ctx.audio.samples.extend(pcm_f32);
        }
        ClientMessage::Flush | ClientMessage::Stop => {
            let ctx = context
                .as_mut()
                .ok_or_else(|| DomainError::Streaming("start must be sent first".to_string()))?;
            state.usecase.process_existing_context(ctx).await?;
            let events = std::mem::take(&mut ctx.events);
            for event in events {
                send_message(socket, ServerMessage::from(event)).await?;
            }
        }
        ClientMessage::Ping => {
            send_message(socket, ServerMessage::Pong).await?;
        }
    }
    Ok(())
}

async fn send_message(socket: &mut WebSocket, message: ServerMessage) -> Result<(), DomainError> {
    let payload = serde_json::to_string(&ServerEnvelope::new(message))
        .map_err(|err| DomainError::Streaming(format!("serialization error: {err}")))?;
    socket
        .send(Message::Text(payload.into()))
        .await
        .map_err(|err| DomainError::Streaming(format!("send error: {err}")))
}

pub struct JsonStreamingProtocol;

impl asr_domain::StreamingProtocolPort for JsonStreamingProtocol {
    fn version(&self) -> u32 {
        PROTOCOL_VERSION
    }

    fn to_wire(&self, event: &DomainEvent) -> Result<String, DomainError> {
        serde_json::to_string(&ServerEnvelope::new(ServerMessage::from(event.clone())))
            .map_err(|err| DomainError::Streaming(format!("serialization error: {err}")))
    }
}
