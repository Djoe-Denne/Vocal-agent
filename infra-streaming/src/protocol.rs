use asr_domain::{DomainEvent, LanguageTag, Transcript, WordTiming};
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientEnvelope {
    pub version: u32,
    #[serde(flatten)]
    pub message: ClientMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ClientMessage {
    Start {
        session_id: Option<String>,
        language_hint: Option<LanguageTag>,
    },
    AudioFrame {
        pcm_f32: Vec<f32>,
    },
    Flush,
    Stop,
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerEnvelope {
    pub version: u32,
    #[serde(flatten)]
    pub message: ServerMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ServerMessage {
    Ready {
        session_id: String,
    },
    PartialTranscript {
        transcript: Transcript,
    },
    FinalTranscript {
        transcript: Transcript,
    },
    AlignmentUpdate {
        words: Vec<WordTiming>,
    },
    Error {
        message: String,
    },
    Pong,
}

impl From<DomainEvent> for ServerMessage {
    fn from(value: DomainEvent) -> Self {
        match value {
            DomainEvent::Ready { session_id } => ServerMessage::Ready { session_id },
            DomainEvent::PartialTranscript { transcript } => {
                ServerMessage::PartialTranscript { transcript }
            }
            DomainEvent::FinalTranscript { transcript } => ServerMessage::FinalTranscript { transcript },
            DomainEvent::AlignmentUpdate { words } => ServerMessage::AlignmentUpdate { words },
            DomainEvent::Error { message } => ServerMessage::Error { message },
        }
    }
}

impl ServerEnvelope {
    pub fn new(message: ServerMessage) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ClientEnvelope, ClientMessage, PROTOCOL_VERSION, ServerEnvelope, ServerMessage};

    #[test]
    fn protocol_round_trip() {
        let raw = serde_json::to_string(&ClientEnvelope {
            version: PROTOCOL_VERSION,
            message: ClientMessage::Ping,
        })
        .expect("serializes");
        let decoded: ClientEnvelope = serde_json::from_str(&raw).expect("deserializes");
        assert_eq!(decoded.version, PROTOCOL_VERSION);
    }

    #[test]
    fn outbound_has_version() {
        let env = ServerEnvelope::new(ServerMessage::Pong);
        assert_eq!(env.version, PROTOCOL_VERSION);
    }
}
