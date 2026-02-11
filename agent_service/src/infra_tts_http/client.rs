use anyhow::Context;
use reqwest::blocking::Client;
use serde_json::json;

use crate::application::config::TtsClientConfig;
use crate::domain::models::{TtsSynthesis, TtsSynthesizeRequest};
use crate::domain::ports::TtsPort;

pub struct TtsHttpClient {
    base_url: String,
    voice_preset: Option<String>,
    voice_sample: Option<String>,
    client: Client,
}

impl TtsHttpClient {
    pub fn from_config(config: &TtsClientConfig) -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(config.timeout())
            .build()
            .context("Failed to build TTS HTTP client")?;

        Ok(Self {
            base_url: config.base_url.clone(),
            voice_preset: config.voice_preset.clone(),
            voice_sample: config.voice_sample.clone(),
            client,
        })
    }
}

impl TtsPort for TtsHttpClient {
    fn synthesize(&self, request: &TtsSynthesizeRequest) -> anyhow::Result<TtsSynthesis> {
        let endpoint = format!(
            "{}/v1/audio/speech",
            self.base_url.trim_end_matches('/')
        );
        eprintln!(
            "[agent_service][tts_http] request endpoint={} input_len={} input_preview={} voice_preset={:?} voice_sample={:?}",
            endpoint,
            request.text.len(),
            preview_text(&request.text, 120),
            self.voice_preset,
            self.voice_sample
        );

        let mut payload = json!({
            "input": request.text,
        });

        if let Some(preset) = &self.voice_preset {
            payload["voice_preset"] = json!(preset);
        }
        if let Some(sample) = &self.voice_sample {
            payload["voice_sample"] = json!(sample);
        }

        let response = self
            .client
            .post(&endpoint)
            .json(&payload)
            .send()
            .context("Failed to call TTS /v1/audio/speech endpoint")?;
        eprintln!(
            "[agent_service][tts_http] response status={}",
            response.status()
        );

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            anyhow::bail!(
                "TTS /v1/audio/speech failed (status {}): {}",
                status,
                body.trim()
            );
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("audio/wav")
            .to_owned();

        let audio_data = response
            .bytes()
            .context("Failed to read TTS audio response bytes")?
            .to_vec();
        eprintln!(
            "[agent_service][tts_http] parsed content_type={} audio_bytes={}",
            content_type,
            audio_data.len()
        );

        Ok(TtsSynthesis {
            audio_data,
            content_type,
        })
    }
}

fn preview_text(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    let mut count = 0usize;
    for ch in text.chars() {
        if count >= max_chars {
            out.push_str("...");
            break;
        }
        out.push(ch);
        count += 1;
    }
    out
}
