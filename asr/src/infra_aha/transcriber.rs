//! Aha / Qwen3-ASR engine adapter.
//!
//! Implements the [`AsrEnginePort`] domain port using the `aha` crate's
//! `Qwen3AsrGenerateModel` (candle-based, CUDA-accelerated).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use aha::models::qwen3_asr::generate::Qwen3AsrGenerateModel;
use aha::models::GenerateModel;
use aha_openai_dive::v1::resources::chat::ChatCompletionParameters;

use crate::application::config::EngineConfig;
use crate::domain::models::{ResolvedModel, TranscriptionRequest, TranscriptionResult, TranscriptionTiming};
use crate::domain::ports::AsrEnginePort;

/// Concrete adapter wrapping the Aha / Qwen3 ASR model.
///
/// Lazy-loads the model on first call and caches it for subsequent requests.
/// The model directory comes from the [`ResolvedModel`] passed by the use case.
pub struct AhaTranscriber<'a> {
    /// Engine configuration (device, etc.). Reserved for future device selection.
    #[allow(dead_code)]
    config: EngineConfig,
    /// Cached model instance (loaded on first use).
    model: Option<Qwen3AsrGenerateModel<'a>>,
    /// The model directory for which the cached model was loaded.
    loaded_model_dir: Option<PathBuf>,
}

impl<'a> AhaTranscriber<'a> {
    pub fn new(config: EngineConfig) -> Self {
        Self {
            config,
            model: None,
            loaded_model_dir: None,
        }
    }

    /// Ensure the model is loaded for the given directory.
    ///
    /// Reloads if the directory changed since last load.
    fn ensure_model_loaded(
        &mut self,
        model_dir: &Path,
    ) -> anyhow::Result<()> {
        // If already loaded for this directory, skip.
        if let Some(ref loaded_dir) = self.loaded_model_dir {
            if loaded_dir == model_dir {
                return Ok(());
            }
            // Different directory — drop old model.
            self.model = None;
            self.loaded_model_dir = None;
        }

        let model_dir_str = model_dir.to_str().ok_or_else(|| {
            anyhow::anyhow!("Invalid model directory path: {:?}", model_dir)
        })?;

        println!("Loading Qwen3 ASR model from {}...", model_dir_str);
        let start = Instant::now();

        let model = Qwen3AsrGenerateModel::init(model_dir_str, None, None)?;

        println!(
            "Model loaded in {:.1}s",
            start.elapsed().as_secs_f64()
        );
        self.model = Some(model);
        self.loaded_model_dir = Some(model_dir.to_path_buf());
        Ok(())
    }

    /// Build an OpenAI-compatible `ChatCompletionParameters` for an audio file.
    fn build_chat_request(audio_path: &Path) -> anyhow::Result<ChatCompletionParameters> {
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

impl<'a> AsrEnginePort for AhaTranscriber<'a> {
    fn transcribe(
        &mut self,
        model: &ResolvedModel,
        request: &TranscriptionRequest,
    ) -> anyhow::Result<TranscriptionResult> {
        // Ensure model is loaded (lazy, cached).
        self.ensure_model_loaded(&model.root_dir)?;

        let engine_model = self
            .model
            .as_mut()
            .expect("model guaranteed loaded by ensure_model_loaded");

        anyhow::ensure!(
            request.audio_path.exists(),
            "Audio file not found: {}",
            request.audio_path.display()
        );

        let start = Instant::now();
        let chat_request = Self::build_chat_request(&request.audio_path)?;
        let response = engine_model.generate(chat_request)?;

        let text = response
            .choices
            .first()
            .and_then(|c| c.message.text())
            .map(|s| s.trim().to_owned())
            .unwrap_or_default();

        let inference_ms = start.elapsed().as_secs_f64() * 1000.0;

        Ok(TranscriptionResult {
            text,
            sample_rate: None,
            audio_duration_secs: None,
            metadata: HashMap::new(),
            timings: TranscriptionTiming {
                inference_ms,
                ..Default::default()
            },
            warnings: Vec::new(),
        })
    }
}
