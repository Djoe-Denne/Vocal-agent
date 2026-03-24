use tempo_domain::{DomainError, TempoPipelineContext, TempoPipelineStage};

pub struct TempoPipelineEngine {
    stages: Vec<Box<dyn TempoPipelineStage>>,
}

impl TempoPipelineEngine {
    pub fn new(stages: Vec<Box<dyn TempoPipelineStage>>) -> Self {
        Self { stages }
    }

    pub fn run(&self, context: &mut TempoPipelineContext) -> Result<(), DomainError> {
        for stage in &self.stages {
            let name = stage.name();
            tracing::debug!(stage = name, "stage_start");
            match stage.execute(context) {
                Ok(()) => {
                    tracing::debug!(stage = name, "stage_end");
                }
                Err(err) => {
                    tracing::error!(stage = name, error = %err, "stage_error");
                    return Err(err);
                }
            }
        }
        Ok(())
    }

    pub fn stage_names(&self) -> Vec<&'static str> {
        self.stages.iter().map(|s| s.name()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempo_domain::{DomainError, TempoPipelineContext, TempoPipelineStage};

    struct CountingStage {
        id: &'static str,
    }

    impl TempoPipelineStage for CountingStage {
        fn name(&self) -> &'static str {
            self.id
        }
        fn execute(&self, context: &mut TempoPipelineContext) -> Result<(), DomainError> {
            context.samples.push(1.0);
            Ok(())
        }
    }

    struct FailingStage;

    impl TempoPipelineStage for FailingStage {
        fn name(&self) -> &'static str {
            "failing"
        }
        fn execute(&self, _context: &mut TempoPipelineContext) -> Result<(), DomainError> {
            Err(DomainError::internal_error("deliberate failure"))
        }
    }

    fn empty_context() -> TempoPipelineContext {
        TempoPipelineContext::new(Vec::new(), 16_000, Vec::new(), Vec::new())
    }

    #[test]
    fn stages_execute_in_order() {
        let engine = TempoPipelineEngine::new(vec![
            Box::new(CountingStage { id: "a" }),
            Box::new(CountingStage { id: "b" }),
            Box::new(CountingStage { id: "c" }),
        ]);
        let mut ctx = empty_context();
        engine.run(&mut ctx).expect("should succeed");
        assert_eq!(ctx.samples.len(), 3);
    }

    #[test]
    fn fail_fast_stops_on_first_error() {
        let engine = TempoPipelineEngine::new(vec![
            Box::new(CountingStage { id: "a" }),
            Box::new(FailingStage),
            Box::new(CountingStage { id: "c" }),
        ]);
        let mut ctx = empty_context();
        let result = engine.run(&mut ctx);
        assert!(result.is_err());
        assert_eq!(ctx.samples.len(), 1, "only stage 'a' should have run");
    }

    #[test]
    fn stage_names_returns_ordered_names() {
        let engine = TempoPipelineEngine::new(vec![
            Box::new(CountingStage { id: "x" }),
            Box::new(CountingStage { id: "y" }),
        ]);
        assert_eq!(engine.stage_names(), vec!["x", "y"]);
    }

    #[test]
    fn empty_pipeline_succeeds() {
        let engine = TempoPipelineEngine::new(Vec::new());
        let mut ctx = empty_context();
        engine.run(&mut ctx).expect("empty pipeline should succeed");
    }
}
