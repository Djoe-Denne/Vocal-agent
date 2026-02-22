use asr_domain::{
    AlignmentOutput, AlignmentPort, AlignmentRequest, DomainError, DomainEvent, PipelineContext,
    PipelineStage, WordTiming,
};
use async_trait::async_trait;

fn is_control_token(token: &str) -> bool {
    token.starts_with("[_") && token.ends_with(']')
}

pub struct SimpleForcedAligner {
    min_word_duration_ms: u64,
}

impl SimpleForcedAligner {
    pub fn new(min_word_duration_ms: u64) -> Self {
        Self {
            min_word_duration_ms,
        }
    }
}

#[async_trait]
impl AlignmentPort for SimpleForcedAligner {
    async fn align(&self, request: AlignmentRequest) -> Result<AlignmentOutput, DomainError> {
        let mut words = Vec::new();

        for segment in &request.transcript.segments {
            if !segment.tokens.is_empty() {
                for token in &segment.tokens {
                    let trimmed = token.text.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if is_control_token(trimmed) {
                        continue;
                    }
                    words.push(WordTiming {
                        word: trimmed.to_string(),
                        start_ms: token.start_ms,
                        end_ms: token
                            .end_ms
                            .max(token.start_ms.saturating_add(self.min_word_duration_ms)),
                        confidence: token.confidence,
                    });
                }
                continue;
            }

            let segment_words: Vec<&str> = segment.text.split_whitespace().collect();
            if segment_words.is_empty() {
                continue;
            }
            let total = segment.end_ms.saturating_sub(segment.start_ms);
            let each = (total / segment_words.len() as u64).max(self.min_word_duration_ms);
            for (idx, word) in segment_words.into_iter().enumerate() {
                let start = segment.start_ms.saturating_add(idx as u64 * each);
                let end = if idx == 0 {
                    start.saturating_add(each)
                } else {
                    start
                        .saturating_add(each / 2)
                        .max(start.saturating_add(self.min_word_duration_ms))
                };
                words.push(WordTiming {
                    word: word.to_string(),
                    start_ms: start,
                    end_ms: end,
                    confidence: 0.8,
                });
            }
        }

        Ok(AlignmentOutput { words })
    }
}

pub struct ForcedAlignmentStage {
    aligner: Box<dyn AlignmentPort>,
}

impl ForcedAlignmentStage {
    pub fn new(aligner: Box<dyn AlignmentPort>) -> Self {
        Self { aligner }
    }
}

#[async_trait]
impl PipelineStage for ForcedAlignmentStage {
    fn name(&self) -> &'static str {
        "forced-alignment"
    }

    async fn execute(&self, context: &mut PipelineContext) -> Result<(), DomainError> {
        let transcript = context
            .transcript
            .clone()
            .ok_or_else(|| DomainError::internal_error("no transcript available"))?;
        let aligned = self.aligner.align(AlignmentRequest { transcript }).await?;
        context.aligned_words = aligned.words.clone();
        context.events.push(DomainEvent::AlignmentUpdate {
            words: aligned.words,
        });
        Ok(())
    }
}
