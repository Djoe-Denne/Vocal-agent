use crate::domain::models::{
    AgentResponse, AsrTranscribeRequest, AsrTranscription, TtsSynthesis, TtsSynthesizeRequest,
};

pub trait AsrPort: Send + Sync {
    fn transcribe(&self, request: &AsrTranscribeRequest) -> anyhow::Result<AsrTranscription>;
}

pub trait ConversationalAgentPort: Send + Sync {
    fn send_text(&self, text: &str) -> anyhow::Result<AgentResponse>;
}

pub trait TtsPort: Send + Sync {
    fn synthesize(&self, request: &TtsSynthesizeRequest) -> anyhow::Result<TtsSynthesis>;
}
