use std::{future::Future, sync::Arc, time::Duration};

use anyhow::{anyhow, Error};
use orchestration_application::{
    AsrCommandRegistryFactory, AsrUseCase, AsrUseCaseImpl, PipelineDefinition, PipelineEngine,
    PipelineStepLoader, PipelineStepSpec,
};
use orchestration_configuration::{AppConfig, GrpcEndpointConfig, PipelineDefinitionConfig};
use orchestration_domain::{DomainError, PipelineStage};
use orchestration_http_server::create_app_routes;
use orchestration_infra::DiagnosticDumpStage;
use orchestration_infra::SnapshotOriginalTimingsStage;
use orchestration_infra::SwapTtsAudioStage;
use orchestration_infra_alignment::{connect_alignment_client, AlignmentEnrichStage};
use orchestration_infra_asr::{connect_asr_client, AsrTranscribeStage};
use orchestration_infra_audio::{connect_audio_client, AudioTransformStage};
use orchestration_infra_tempo::TempoMatchStage;
use orchestration_infra_tts_rest::TtsRestSynthesizeStage;
use rustycog_command::GenericCommandService;
use rustycog_config::ServerConfig;
use rustycog_http::{AppState, UserIdExtractor};

pub async fn build_and_run(config: AppConfig, server_config: ServerConfig) -> Result<(), Error> {
    let app = Application::new(config).await?;
    app.run(server_config).await
}

pub struct Application {
    pub config: AppConfig,
    pub state: AppState,
}

impl Application {
    pub async fn new(config: AppConfig) -> Result<Self, Error> {
        let selected = config.service.pipeline.selected.clone();
        let definition = config
            .service
            .pipeline
            .definitions
            .get(&selected)
            .ok_or_else(|| anyhow!("missing pipeline definition `{selected}`"))?;
        let pipeline_definition = build_pipeline_definition(definition);

        let audio_client = connect_with_retry("audio", || async {
            connect_audio_client(
                &grpc_endpoint_uri(&config.service.audio),
                connect_timeout(&config.service.audio),
                config.service.audio.max_decoding_message_bytes,
                config.service.audio.max_encoding_message_bytes,
            )
            .await
        })
        .await?;
        let asr_client = connect_with_retry("asr", || async {
            connect_asr_client(
                &grpc_endpoint_uri(&config.service.asr),
                connect_timeout(&config.service.asr),
                config.service.asr.max_decoding_message_bytes,
                config.service.asr.max_encoding_message_bytes,
            )
            .await
        })
        .await?;
        let alignment_client = connect_with_retry("alignment", || async {
            connect_alignment_client(
                &grpc_endpoint_uri(&config.service.alignment),
                connect_timeout(&config.service.alignment),
                config.service.alignment.max_decoding_message_bytes,
                config.service.alignment.max_encoding_message_bytes,
            )
            .await
        })
        .await?;
        let audio_stage: Arc<dyn PipelineStage> = Arc::new(AudioTransformStage::new(
            audio_client,
            request_timeout(&config.service.audio),
            None,
        ));
        let asr_stage: Arc<dyn PipelineStage> = Arc::new(AsrTranscribeStage::new(
            asr_client,
            request_timeout(&config.service.asr),
        ));
        let alignment_stage: Arc<dyn PipelineStage> = Arc::new(AlignmentEnrichStage::new(
            alignment_client,
            request_timeout(&config.service.alignment),
        ));
        let tts_stage: Arc<dyn PipelineStage> = Arc::new(TtsRestSynthesizeStage::new(
            format!("{}/v1/audio/speech", grpc_endpoint_uri(&config.service.tts)),
            request_timeout(&config.service.tts),
        ));
        let snapshot_stage: Arc<dyn PipelineStage> =
            Arc::new(SnapshotOriginalTimingsStage::new());
        let swap_stage: Arc<dyn PipelineStage> = Arc::new(SwapTtsAudioStage::new());
        let dump_dir = std::path::PathBuf::from("./debug-dumps");
        let dump_original: Arc<dyn PipelineStage> =
            Arc::new(DiagnosticDumpStage::new("01_original", &dump_dir));
        let dump_tts_audio: Arc<dyn PipelineStage> =
            Arc::new(DiagnosticDumpStage::new("02_tts_audio", &dump_dir));
        let dump_tts_aligned: Arc<dyn PipelineStage> =
            Arc::new(DiagnosticDumpStage::new("03_tts_aligned", &dump_dir));
        let dump_tempo_result: Arc<dyn PipelineStage> =
            Arc::new(DiagnosticDumpStage::new("04_tempo_result", &dump_dir));
        let dump_final: Arc<dyn PipelineStage> =
            Arc::new(DiagnosticDumpStage::new("05_final", &dump_dir));
        let tempo_stage: Arc<dyn PipelineStage> = Arc::new(TempoMatchStage::new());
        let loader = GrpcPipelineStepLoader {
            audio_transform: audio_stage,
            asr_transcribe: asr_stage,
            alignment_enrich: alignment_stage,
            tts_synthesize: tts_stage,
            snapshot_original_timings: snapshot_stage,
            swap_tts_audio: swap_stage,
            tempo_match: tempo_stage,
            dump_original,
            dump_tts_audio,
            dump_tts_aligned,
            dump_tempo_result,
            dump_final,
        };
        let pipeline = PipelineEngine::from_definition(&pipeline_definition, &loader)?;

        let usecase: Arc<dyn AsrUseCase> = Arc::new(AsrUseCaseImpl::new(pipeline, 16_000));
        let registry = AsrCommandRegistryFactory::create_registry(usecase);
        let command_service = Arc::new(GenericCommandService::new(Arc::new(registry)));
        let state = AppState::new(command_service, UserIdExtractor::new());

        Ok(Self { config, state })
    }

    pub async fn run(self, server_config: ServerConfig) -> Result<(), Error> {
        create_app_routes(self.state, server_config)
            .await
            .map_err(|err| anyhow!("orchestration http server failed: {err}"))
    }
}

struct GrpcPipelineStepLoader {
    audio_transform: Arc<dyn PipelineStage>,
    asr_transcribe: Arc<dyn PipelineStage>,
    alignment_enrich: Arc<dyn PipelineStage>,
    tts_synthesize: Arc<dyn PipelineStage>,
    snapshot_original_timings: Arc<dyn PipelineStage>,
    swap_tts_audio: Arc<dyn PipelineStage>,
    tempo_match: Arc<dyn PipelineStage>,
    dump_original: Arc<dyn PipelineStage>,
    dump_tts_audio: Arc<dyn PipelineStage>,
    dump_tts_aligned: Arc<dyn PipelineStage>,
    dump_tempo_result: Arc<dyn PipelineStage>,
    dump_final: Arc<dyn PipelineStage>,
}

impl PipelineStepLoader for GrpcPipelineStepLoader {
    fn load_step(&self, step: &PipelineStepSpec) -> Result<Arc<dyn PipelineStage>, DomainError> {
        match step.name.as_str() {
            "audio_transform" => Ok(self.audio_transform.clone()),
            "asr_transcribe" | "asr_transcribe_tts" | "asr_transcribe_result" => {
                Ok(self.asr_transcribe.clone())
            }
            "alignment_enrich" | "alignment_enrich_tts" | "alignment_enrich_result" => {
                Ok(self.alignment_enrich.clone())
            }
            "tts_synthesize" => Ok(self.tts_synthesize.clone()),
            "snapshot_original_timings" => Ok(self.snapshot_original_timings.clone()),
            "swap_tts_audio" => Ok(self.swap_tts_audio.clone()),
            "tempo_match" => Ok(self.tempo_match.clone()),
            "dump_original" => Ok(self.dump_original.clone()),
            "dump_tts_audio" => Ok(self.dump_tts_audio.clone()),
            "dump_tts_aligned" => Ok(self.dump_tts_aligned.clone()),
            "dump_tempo_result" => Ok(self.dump_tempo_result.clone()),
            "dump_final" => Ok(self.dump_final.clone()),
            _ => Err(DomainError::internal_error(&format!(
                "unknown pipeline step `{}`",
                step.name
            ))),
        }
    }
}

fn build_pipeline_definition(definition: &PipelineDefinitionConfig) -> PipelineDefinition {
    PipelineDefinition {
        pre: definition
            .pre
            .iter()
            .map(|step| PipelineStepSpec::new(step.name()))
            .collect(),
        transcription: PipelineStepSpec::new(definition.transcription.name()),
        post: definition
            .post
            .iter()
            .map(|step| PipelineStepSpec::new(step.name()))
            .collect(),
    }
}

fn grpc_endpoint_uri(config: &GrpcEndpointConfig) -> String {
    let scheme = if config.tls_enabled { "https" } else { "http" };
    format!("{scheme}://{}:{}", config.host, config.port)
}

fn connect_timeout(config: &GrpcEndpointConfig) -> Duration {
    Duration::from_millis(config.connect_timeout_ms.max(1))
}

fn request_timeout(config: &GrpcEndpointConfig) -> Duration {
    Duration::from_millis(config.request_timeout_ms.max(1))
}

async fn connect_with_retry<C, F, Fut>(service: &str, mut connect_fn: F) -> Result<C, Error>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<C, DomainError>>,
{
    let mut last_error = None;
    for _ in 0..20 {
        match connect_fn().await {
            Ok(client) => return Ok(client),
            Err(err) => {
                last_error = Some(err);
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
    Err(anyhow!(
        "failed to connect to {service} service after retries: {}",
        last_error
            .map(|err| err.to_string())
            .unwrap_or_else(|| "unknown connection error".to_string())
    ))
}

#[cfg(test)]
mod tests {
    use orchestration_configuration::{PipelineDefinitionConfig, PipelineStepRef};

    use super::*;

    struct FakeStage {
        id: &'static str,
    }

    #[async_trait::async_trait]
    impl PipelineStage for FakeStage {
        fn name(&self) -> &'static str {
            self.id
        }

        async fn execute(
            &self,
            _context: &mut orchestration_domain::PipelineContext,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[test]
    fn pipeline_definition_preserves_step_order() {
        let definition = PipelineDefinitionConfig {
            pre: vec![PipelineStepRef::Name("audio_transform".to_string())],
            transcription: PipelineStepRef::WithName {
                name: "asr_transcribe".to_string(),
            },
            post: vec![
                PipelineStepRef::Name("alignment_enrich".to_string()),
                PipelineStepRef::Name("tts_synthesize".to_string()),
            ],
        };
        let built = build_pipeline_definition(&definition);
        let ordered = built.ordered_steps();
        assert_eq!(ordered[0].name, "audio_transform");
        assert_eq!(ordered[1].name, "asr_transcribe");
        assert_eq!(ordered[2].name, "alignment_enrich");
        assert_eq!(ordered[3].name, "tts_synthesize");
    }

    fn make_fake_stage(id: &'static str) -> Arc<dyn PipelineStage> {
        Arc::new(FakeStage { id })
    }

    fn make_test_loader() -> GrpcPipelineStepLoader {
        GrpcPipelineStepLoader {
            audio_transform: make_fake_stage("audio_transform"),
            asr_transcribe: make_fake_stage("asr_transcribe"),
            alignment_enrich: make_fake_stage("alignment_enrich"),
            tts_synthesize: make_fake_stage("tts_synthesize"),
            snapshot_original_timings: make_fake_stage("snapshot_original_timings"),
            swap_tts_audio: make_fake_stage("swap_tts_audio"),
            tempo_match: make_fake_stage("tempo_match"),
            dump_original: make_fake_stage("diagnostic_dump"),
            dump_tts_audio: make_fake_stage("diagnostic_dump"),
            dump_tts_aligned: make_fake_stage("diagnostic_dump"),
            dump_tempo_result: make_fake_stage("diagnostic_dump"),
            dump_final: make_fake_stage("diagnostic_dump"),
        }
    }

    #[test]
    fn loader_maps_remote_step_names() {
        let loader = make_test_loader();

        assert_eq!(
            loader
                .load_step(&PipelineStepSpec::new("audio_transform"))
                .unwrap()
                .name(),
            "audio_transform"
        );
        assert_eq!(
            loader
                .load_step(&PipelineStepSpec::new("asr_transcribe"))
                .unwrap()
                .name(),
            "asr_transcribe"
        );
        assert_eq!(
            loader
                .load_step(&PipelineStepSpec::new("alignment_enrich"))
                .unwrap()
                .name(),
            "alignment_enrich"
        );
        assert_eq!(
            loader
                .load_step(&PipelineStepSpec::new("tts_synthesize"))
                .unwrap()
                .name(),
            "tts_synthesize"
        );
        assert_eq!(
            loader
                .load_step(&PipelineStepSpec::new("snapshot_original_timings"))
                .unwrap()
                .name(),
            "snapshot_original_timings"
        );
        assert_eq!(
            loader
                .load_step(&PipelineStepSpec::new("swap_tts_audio"))
                .unwrap()
                .name(),
            "swap_tts_audio"
        );
        assert_eq!(
            loader
                .load_step(&PipelineStepSpec::new("tempo_match"))
                .unwrap()
                .name(),
            "tempo_match"
        );
        assert!(loader
            .load_step(&PipelineStepSpec::new("unknown_step"))
            .is_err());
    }

    #[test]
    fn loader_aliases_reuse_same_stage() {
        let loader = make_test_loader();

        let asr = loader
            .load_step(&PipelineStepSpec::new("asr_transcribe"))
            .unwrap();
        let asr_tts = loader
            .load_step(&PipelineStepSpec::new("asr_transcribe_tts"))
            .unwrap();
        assert!(Arc::ptr_eq(&asr, &asr_tts));
        let asr_result = loader
            .load_step(&PipelineStepSpec::new("asr_transcribe_result"))
            .unwrap();
        assert!(Arc::ptr_eq(&asr, &asr_result));

        let align = loader
            .load_step(&PipelineStepSpec::new("alignment_enrich"))
            .unwrap();
        let align_tts = loader
            .load_step(&PipelineStepSpec::new("alignment_enrich_tts"))
            .unwrap();
        assert!(Arc::ptr_eq(&align, &align_tts));
        let align_result = loader
            .load_step(&PipelineStepSpec::new("alignment_enrich_result"))
            .unwrap();
        assert!(Arc::ptr_eq(&align, &align_result));
    }
}
