//! OpenClaw GAIAgent adapter (HTTP-based).

use std::time::Instant;

use anyhow::Context;
use reqwest::blocking::Client;
use serde_json::json;

use crate::application::config::OpenClawConfig;
use crate::domain::pipeline::{PostProcessorContext, StageTiming};
use crate::domain::ports::{GAIAgentPort, PostProcessor};

/// GAIAgent adapter that calls the OpenClaw HTTP API.
pub struct OpenClawHttpAgent {
    base_url: String,
    model: String,
    token: String,
    client: Client,
}

impl OpenClawHttpAgent {
    pub fn from_config(config: &OpenClawConfig) -> anyhow::Result<Self> {
        let model = config
            .model
            .clone()
            .ok_or_else(|| anyhow::anyhow!("openclaw.model is required"))?;
        let token = config
            .token
            .clone()
            .ok_or_else(|| anyhow::anyhow!("OPENCLAW_TOKEN is required"))?;

        Ok(Self {
            base_url: config.base_url.clone(),
            model,
            token,
            client: Client::new(),
        })
    }
}

impl GAIAgentPort for OpenClawHttpAgent {
    fn send_transcription(&self, ctx: &PostProcessorContext) -> anyhow::Result<()> {
        let endpoint = format!(
            "{}/v1/chat/completions",
            self.base_url.trim_end_matches('/')
        );
        let payload = json!({
            "model": self.model,
            "messages": [
                { "role": "user", "content": ctx.text }
            ],
            "stream": false
        });

        let response = self
            .client
            .post(&endpoint)
            .bearer_auth(&self.token)
            .json(&payload)
            .send()
            .with_context(|| "Failed to call OpenClaw HTTP API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            anyhow::bail!(
                "OpenClaw HTTP call failed (status {}): {}",
                status,
                body.trim()
            );
        }

        Ok(())
    }
}

/// Post-processor that forwards transcriptions to OpenClaw.
pub struct OpenClawPostProcessor {
    agent: Box<dyn GAIAgentPort>,
}

impl OpenClawPostProcessor {
    pub fn new(config: OpenClawConfig) -> anyhow::Result<Self> {
        let agent = OpenClawHttpAgent::from_config(&config)?;
        Ok(Self {
            agent: Box::new(agent),
        })
    }
}

impl PostProcessor for OpenClawPostProcessor {
    fn name(&self) -> &str {
        "openclaw"
    }

    fn process(
        &self,
        mut ctx: PostProcessorContext,
    ) -> anyhow::Result<PostProcessorContext> {
        let start = Instant::now();
        self.agent
            .send_transcription(&ctx)
            .with_context(|| "OpenClaw delivery failed")?;

        ctx.stage_timings.push(StageTiming {
            stage_name: self.name().to_owned(),
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
        });

        Ok(ctx)
    }
}
