use std::time::Instant;

use crate::domain::models::{
    AsrTranscribeRequest, ProcessResult, ProcessTiming,
};
use crate::domain::ports::{AsrPort, ConversationalAgentPort};

pub struct ProcessAudioUseCase {
    asr: Box<dyn AsrPort>,
    conversational_agent: Option<Box<dyn ConversationalAgentPort>>,
}

impl ProcessAudioUseCase {
    pub fn new(
        asr: Box<dyn AsrPort>,
        conversational_agent: Option<Box<dyn ConversationalAgentPort>>,
    ) -> Self {
        Self {
            asr,
            conversational_agent,
        }
    }

    pub fn execute(
        &mut self,
        audio_path: std::path::PathBuf,
        language: Option<String>,
    ) -> anyhow::Result<ProcessResult> {
        let total_start = Instant::now();
        let asr_start = Instant::now();

        let transcription = self.asr.transcribe(&AsrTranscribeRequest {
            audio_path,
            language,
        })?;

        let asr_ms = asr_start.elapsed().as_secs_f64() * 1000.0;
        let agent_start = Instant::now();

        let agent_response = if let Some(agent) = &self.conversational_agent {
            agent.send_text(&transcription.text)?.text
        } else {
            None
        };

        let agent_ms = agent_start.elapsed().as_secs_f64() * 1000.0;
        let total_ms = total_start.elapsed().as_secs_f64() * 1000.0;

        Ok(ProcessResult {
            transcription: transcription.text,
            agent_response,
            warnings: transcription.warnings,
            timings: ProcessTiming {
                asr_ms,
                agent_ms,
                total_ms,
            },
        })
    }
}
