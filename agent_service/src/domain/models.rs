use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AsrTranscribeRequest {
    pub audio_path: PathBuf,
    pub language: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AsrTranscription {
    pub text: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AgentResponse {
    pub text: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProcessTiming {
    pub asr_ms: f64,
    pub agent_ms: f64,
    pub total_ms: f64,
}

#[derive(Debug, Clone)]
pub struct ProcessResult {
    pub transcription: String,
    pub agent_response: Option<String>,
    pub warnings: Vec<String>,
    pub timings: ProcessTiming,
}
