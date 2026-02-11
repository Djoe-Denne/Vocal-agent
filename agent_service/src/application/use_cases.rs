use std::time::Instant;

use crate::domain::models::{
    AsrTranscribeRequest, ProcessResult, ProcessTiming, TtsSynthesizeRequest,
};
use crate::domain::ports::{AsrPort, ConversationalAgentPort, TtsPort};

pub struct ProcessAudioUseCase {
    asr: Box<dyn AsrPort>,
    conversational_agent: Option<Box<dyn ConversationalAgentPort>>,
    tts: Option<Box<dyn TtsPort>>,
}

impl ProcessAudioUseCase {
    pub fn new(
        asr: Box<dyn AsrPort>,
        conversational_agent: Option<Box<dyn ConversationalAgentPort>>,
        tts: Option<Box<dyn TtsPort>>,
    ) -> Self {
        Self {
            asr,
            conversational_agent,
            tts,
        }
    }

    pub fn execute(
        &mut self,
        audio_path: std::path::PathBuf,
        language: Option<String>,
    ) -> anyhow::Result<ProcessResult> {
        eprintln!(
            "[agent_service][use_case] start execute audio_path={} language={:?}",
            audio_path.display(),
            language
        );
        let total_start = Instant::now();
        let asr_start = Instant::now();

        let transcription = self.asr.transcribe(&AsrTranscribeRequest {
            audio_path,
            language,
        })?;

        let asr_ms = asr_start.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "[agent_service][use_case] asr done in {:.1}ms text_len={} warnings={}",
            asr_ms,
            transcription.text.len(),
            transcription.warnings.len()
        );
        let agent_start = Instant::now();

        let agent_response = if let Some(agent) = &self.conversational_agent {
            agent.send_text(&transcription.text)?.text
        } else {
            None
        };

        let agent_ms = agent_start.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "[agent_service][use_case] agent done in {:.1}ms response_len={}",
            agent_ms,
            agent_response.as_ref().map(|s| s.len()).unwrap_or(0)
        );
        let tts_start = Instant::now();

        let audio = if let Some(tts) = &self.tts {
            // Synthesize the agent response if available, otherwise the transcription itself
            let text_to_speak = agent_response.as_deref().unwrap_or(&transcription.text);
            if !text_to_speak.is_empty() {
                Some(tts.synthesize(&TtsSynthesizeRequest {
                    text: text_to_speak.to_owned(),
                })?)
            } else {
                None
            }
        } else {
            None
        };

        let tts_ms = tts_start.elapsed().as_secs_f64() * 1000.0;
        let total_ms = total_start.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "[agent_service][use_case] done total={:.1}ms tts={:.1}ms audio_bytes={}",
            total_ms,
            tts_ms,
            audio.as_ref().map(|a| a.audio_data.len()).unwrap_or(0)
        );

        Ok(ProcessResult {
            transcription: transcription.text,
            agent_response,
            audio,
            warnings: transcription.warnings,
            timings: ProcessTiming {
                asr_ms,
                agent_ms,
                tts_ms,
                total_ms,
            },
        })
    }
}
