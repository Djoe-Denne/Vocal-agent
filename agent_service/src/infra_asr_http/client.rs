use anyhow::Context;
use reqwest::blocking::multipart::{Form, Part};
use reqwest::blocking::Client;
use serde::Deserialize;

use crate::application::config::AsrClientConfig;
use crate::domain::models::{AsrTranscribeRequest, AsrTranscription};
use crate::domain::ports::AsrPort;

pub struct AsrHttpClient {
    base_url: String,
    client: Client,
}

impl AsrHttpClient {
    pub fn from_config(config: &AsrClientConfig) -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(config.timeout())
            .build()
            .context("Failed to build ASR HTTP client")?;

        Ok(Self {
            base_url: config.base_url.clone(),
            client,
        })
    }
}

#[derive(Debug, Deserialize)]
struct AsrResponse {
    text: String,
    #[serde(default)]
    warnings: Vec<String>,
}

impl AsrPort for AsrHttpClient {
    fn transcribe(&self, request: &AsrTranscribeRequest) -> anyhow::Result<AsrTranscription> {
        let endpoint = format!(
            "{}/transcribe",
            self.base_url.trim_end_matches('/')
        );

        let file_name = request
            .audio_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("upload.wav")
            .to_owned();

        let audio_bytes = std::fs::read(&request.audio_path).with_context(|| {
            format!(
                "Failed to read audio file {}",
                request.audio_path.display()
            )
        })?;

        let audio_part = Part::bytes(audio_bytes).file_name(file_name);
        let mut form = Form::new().part("file", audio_part);

        if let Some(language) = &request.language {
            form = form.text("language", language.clone());
        }

        let response = self
            .client
            .post(endpoint)
            .multipart(form)
            .send()
            .context("Failed to call ASR /transcribe endpoint")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            anyhow::bail!(
                "ASR /transcribe failed (status {}): {}",
                status,
                body.trim()
            );
        }

        let body: AsrResponse = response
            .json()
            .context("Failed to parse ASR /transcribe JSON response")?;

        Ok(AsrTranscription {
            text: body.text,
            warnings: body.warnings,
        })
    }
}
