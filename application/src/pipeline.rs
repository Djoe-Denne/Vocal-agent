use std::sync::Arc;

use asr_domain::{DomainError, PipelineContext, PipelineStage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineStepSpec {
    pub name: String,
}

impl PipelineStepSpec {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineDefinition {
    pub pre: Vec<PipelineStepSpec>,
    pub transcription: PipelineStepSpec,
    pub post: Vec<PipelineStepSpec>,
}

impl PipelineDefinition {
    pub fn ordered_steps(&self) -> Vec<PipelineStepSpec> {
        let mut ordered = Vec::with_capacity(self.pre.len() + self.post.len() + 1);
        ordered.extend(self.pre.clone());
        ordered.push(self.transcription.clone());
        ordered.extend(self.post.clone());
        ordered
    }
}

pub trait PipelineStepLoader: Send + Sync {
    fn load_step(&self, step: &PipelineStepSpec) -> Result<Arc<dyn PipelineStage>, DomainError>;
}

#[derive(Default)]
pub struct PipelineEngine {
    stages: Vec<Arc<dyn PipelineStage>>,
}

impl PipelineEngine {
    pub fn new(stages: Vec<Arc<dyn PipelineStage>>) -> Self {
        Self { stages }
    }

    pub fn push_stage(&mut self, stage: Arc<dyn PipelineStage>) {
        self.stages.push(stage);
    }

    pub fn from_definition(
        definition: &PipelineDefinition,
        loader: &dyn PipelineStepLoader,
    ) -> Result<Self, DomainError> {
        let mut stages = Vec::new();
        for step in definition.ordered_steps() {
            stages.push(loader.load_step(&step)?);
        }
        Ok(Self::new(stages))
    }

    pub async fn run(&self, context: &mut PipelineContext) -> Result<(), DomainError> {
        for stage in &self.stages {
            tracing::debug!("executing stage={}", stage.name());
            stage.execute(context).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use asr_domain::{
        DomainError, DomainEvent, LanguageTag, PipelineContext, PipelineStage, Transcript,
        TranscriptSegment,
    };
    use async_trait::async_trait;

    use super::PipelineEngine;
    use super::{PipelineDefinition, PipelineStepLoader, PipelineStepSpec};

    struct TestStage {
        id: &'static str,
    }

    #[async_trait]
    impl PipelineStage for TestStage {
        fn name(&self) -> &'static str {
            self.id
        }

        async fn execute(&self, context: &mut PipelineContext) -> Result<(), DomainError> {
            context.events.push(DomainEvent::FinalTranscript {
                transcript: Transcript {
                    language: LanguageTag::En,
                    segments: vec![TranscriptSegment {
                        text: self.id.to_string(),
                        start_ms: 0,
                        end_ms: 10,
                        tokens: Vec::new(),
                    }],
                },
            });
            Ok(())
        }
    }

    #[tokio::test]
    async fn stages_execute_in_order() {
        let pipeline = PipelineEngine::new(vec![
            Arc::new(TestStage { id: "a" }),
            Arc::new(TestStage { id: "b" }),
        ]);
        let mut context = PipelineContext::new("session", None);

        pipeline.run(&mut context).await.expect("pipeline runs");

        assert_eq!(context.events.len(), 2);
        let first = match &context.events[0] {
            DomainEvent::FinalTranscript { transcript } => &transcript.segments[0].text,
            _ => panic!("unexpected event"),
        };
        let second = match &context.events[1] {
            DomainEvent::FinalTranscript { transcript } => &transcript.segments[0].text,
            _ => panic!("unexpected event"),
        };
        assert_eq!(first, "a");
        assert_eq!(second, "b");
    }

    struct TestLoader {
        known: HashMap<String, &'static str>,
    }

    impl PipelineStepLoader for TestLoader {
        fn load_step(&self, step: &PipelineStepSpec) -> Result<Arc<dyn PipelineStage>, DomainError> {
            let id = self
                .known
                .get(&step.name)
                .ok_or_else(|| DomainError::internal_error("unknown step"))?;
            Ok(Arc::new(TestStage { id }))
        }
    }

    #[tokio::test]
    async fn engine_can_be_built_from_definition() {
        let loader = TestLoader {
            known: HashMap::from([
                ("pre".to_string(), "a"),
                ("transcribe".to_string(), "b"),
                ("post".to_string(), "c"),
            ]),
        };
        let definition = PipelineDefinition {
            pre: vec![PipelineStepSpec::new("pre")],
            transcription: PipelineStepSpec::new("transcribe"),
            post: vec![PipelineStepSpec::new("post")],
        };
        let pipeline = PipelineEngine::from_definition(&definition, &loader).expect("pipeline");
        let mut context = PipelineContext::new("session", None);

        pipeline.run(&mut context).await.expect("pipeline runs");

        assert_eq!(context.events.len(), 3);
    }
}
