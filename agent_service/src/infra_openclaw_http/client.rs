use anyhow::Context;
use reqwest::blocking::Client;
use serde_json::{json, Value};

use crate::application::config::OpenClawClientConfig;
use crate::domain::models::AgentResponse;
use crate::domain::ports::ConversationalAgentPort;

pub struct OpenClawHttpClient {
    base_url: String,
    model: String,
    token: String,
    client: Client,
}

impl OpenClawHttpClient {
    pub fn from_config(config: &OpenClawClientConfig) -> anyhow::Result<Self> {
        let model = config
            .model
            .clone()
            .ok_or_else(|| anyhow::anyhow!("openclaw.model is required"))?;
        let token = config
            .token
            .clone()
            .ok_or_else(|| anyhow::anyhow!("OPENCLAW_TOKEN is required"))?;

        let client = Client::builder()
            .timeout(config.timeout())
            .build()
            .context("Failed to build OpenClaw HTTP client")?;

        Ok(Self {
            base_url: config.base_url.clone(),
            model,
            token,
            client,
        })
    }
}

impl ConversationalAgentPort for OpenClawHttpClient {
    fn send_text(&self, text: &str) -> anyhow::Result<AgentResponse> {
        let endpoint = format!(
            "{}/v1/chat/completions",
            self.base_url.trim_end_matches('/')
        );
        eprintln!(
            "[agent_service][openclaw_http] request endpoint={} model={} input_len={} input_preview={}",
            endpoint,
            self.model,
            text.len(),
            preview_text(text, 120)
        );

        let payload = json!({
            "model": self.model,
            "messages": [
                { "role": "user", "content": text }
            ],
            "stream": false
        });

        let response = self
            .client
            .post(endpoint)
            .bearer_auth(&self.token)
            .json(&payload)
            .send()
            .context("Failed to call OpenClaw /v1/chat/completions endpoint")?;
        eprintln!(
            "[agent_service][openclaw_http] response status={}",
            response.status()
        );

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            anyhow::bail!(
                "OpenClaw call failed (status {}): {}",
                status,
                body.trim()
            );
        }

        let body: Value = response
            .json()
            .context("Failed to parse OpenClaw JSON response")?;

        let text = extract_message_content(&body);
        eprintln!(
            "[agent_service][openclaw_http] parsed response_len={} response_preview={}",
            text.as_ref().map(|t| t.len()).unwrap_or(0),
            text.as_deref().map(|t| preview_text(t, 120)).unwrap_or_else(|| "<none>".to_owned())
        );
        Ok(AgentResponse { text })
    }
}

fn extract_message_content(body: &Value) -> Option<String> {
    let content = body
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))?;

    if let Some(text) = content.as_str() {
        return Some(text.to_owned());
    }

    // OpenAI-compatible providers may return structured arrays for content.
    if let Some(parts) = content.as_array() {
        let joined = parts
            .iter()
            .filter_map(|part| part.get("text").and_then(|text| text.as_str()))
            .collect::<Vec<_>>()
            .join("");

        if !joined.is_empty() {
            return Some(joined);
        }
    }

    None
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
