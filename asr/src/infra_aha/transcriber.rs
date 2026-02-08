//! Aha / Qwen3-ASR transcriber adapter.
//!
//! Implements the [`Transcriber`] domain port using the `aha` crate's
//! `Qwen3AsrGenerateModel` (candle-based, CUDA-accelerated).

use std::path::Path;
use std::time::Instant;

use aha::models::qwen3_asr::generate::Qwen3AsrGenerateModel;
use aha::models::GenerateModel;
use aha_openai_dive::v1::resources::chat::ChatCompletionParameters;

use crate::application::config::ModelConfig;
use crate::domain::models::TranscriptionResult;
use crate::domain::ports::Transcriber;

/// Concrete adapter wrapping the Aha / Qwen3 ASR model.
pub struct AhaTranscriber<'a> {
    config: ModelConfig,
    model: Option<Qwen3AsrGenerateModel<'a>>,
}

impl<'a> AhaTranscriber<'a> {
    pub fn new(config: ModelConfig) -> Self {
        Self {
            config,
            model: None,
        }
    }

    /// Build an OpenAI-compatible `ChatCompletionParameters` for an audio file.
    fn build_request(audio_path: &Path) -> anyhow::Result<ChatCompletionParameters> {
        // Normalise to forward slashes — file:// URLs must use '/' (RFC 8089)
        // and backslashes would break JSON escaping on Windows.
        let path_str = audio_path.display().to_string().replace('\\', "/");
        let audio_url = format!("file://{path_str}");
        let json = format!(
            r#"{{
                "model": "qwen3-asr",
                "messages": [
                    {{
                        "role": "user",
                        "content": [
                            {{
                                "type": "audio",
                                "audio_url": {{
                                    "url": "{audio_url}"
                                }}
                            }}
                        ]
                    }}
                ]
            }}"#,
        );
        let params: ChatCompletionParameters = serde_json::from_str(&json)?;
        Ok(params)
    }
}

impl<'a> Transcriber for AhaTranscriber<'a> {
    fn is_loaded(&self) -> bool {
        self.model.is_some()
    }

    fn load_model(&mut self) -> anyhow::Result<()> {
        if self.model.is_some() {
            return Ok(());
        }

        let model_dir = self.config.model_dir.to_str().ok_or_else(|| {
            anyhow::anyhow!("Invalid model directory path: {:?}", self.config.model_dir)
        })?;

        println!("Loading Qwen3 ASR model from {}...", model_dir);
        let start = Instant::now();

        let model = Qwen3AsrGenerateModel::init(model_dir, None, None)?;

        println!(
            "Model loaded in {:.1}s",
            start.elapsed().as_secs_f64()
        );
        self.model = Some(model);
        Ok(())
    }

    fn unload_model(&mut self) -> anyhow::Result<()> {
        if self.model.is_none() {
            return Ok(());
        }
        self.model = None;
        println!("ASR model unloaded");
        Ok(())
    }

    fn transcribe_file(&mut self, audio_path: &Path) -> anyhow::Result<TranscriptionResult> {
        let model = self
            .model
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("ASR model not loaded -- call load_model() first"))?;

        anyhow::ensure!(
            audio_path.exists(),
            "Audio file not found: {}",
            audio_path.display()
        );

        let start = Instant::now();
        let request = Self::build_request(audio_path)?;
        let response = model.generate(request)?;

        let text = response
            .choices
            .first()
            .and_then(|c| c.message.text())
            .map(|s| s.trim().to_owned())
            .unwrap_or_default();

        let duration_secs = start.elapsed().as_secs_f64();

        Ok(TranscriptionResult {
            text,
            duration_secs,
            audio_duration_secs: None,
        })
    }
}
