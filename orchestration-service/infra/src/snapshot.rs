use async_trait::async_trait;
use orchestration_domain::{DomainError, PipelineContext, PipelineStage};
use serde_json::json;

pub struct SnapshotOriginalTimingsStage;

impl SnapshotOriginalTimingsStage {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl PipelineStage for SnapshotOriginalTimingsStage {
    fn name(&self) -> &'static str {
        "snapshot_original_timings"
    }

    async fn execute(&self, context: &mut PipelineContext) -> Result<(), DomainError> {
        let timings_json = serde_json::to_value(&context.aligned_words).map_err(|err| {
            DomainError::internal_error(&format!(
                "failed to serialize aligned_words for snapshot: {err}"
            ))
        })?;

        let transcript_text = context
            .transcript
            .as_ref()
            .map(|t| {
                t.segments
                    .iter()
                    .map(|s| s.text.trim())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();

        context.set_extension("original.timings", timings_json);
        context.set_extension("original.transcript", json!(transcript_text));

        tracing::debug!(
            aligned_word_count = context.aligned_words.len(),
            transcript_text_len = transcript_text.len(),
            "snapshotted original timings and transcript"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchestration_domain::{LanguageTag, PipelineContext, Transcript, TranscriptSegment, WordTiming};

    #[tokio::test]
    async fn snapshot_stores_timings_in_extensions() {
        let stage = SnapshotOriginalTimingsStage::new();
        let mut context = PipelineContext::new("session", None);
        context.aligned_words = vec![WordTiming {
            word: "hello".to_string(),
            start_ms: 0,
            end_ms: 500,
            confidence: 0.95,
        }];
        context.transcript = Some(Transcript {
            language: LanguageTag::En,
            segments: vec![TranscriptSegment {
                text: "hello world".to_string(),
                start_ms: 0,
                end_ms: 1000,
                tokens: vec![],
            }],
        });

        stage.execute(&mut context).await.expect("stage runs");

        assert!(context.extension("original.timings").is_some());
        assert_eq!(
            context.extension("original.transcript").and_then(|v| v.as_str()),
            Some("hello world")
        );
    }
}
