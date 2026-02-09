use crate::domain::models::{AgentResponse, AsrTranscribeRequest, AsrTranscription};

pub trait AsrPort: Send + Sync {
    fn transcribe(&self, request: &AsrTranscribeRequest) -> anyhow::Result<AsrTranscription>;
}

pub trait ConversationalAgentPort: Send + Sync {
    fn send_text(&self, text: &str) -> anyhow::Result<AgentResponse>;
}
