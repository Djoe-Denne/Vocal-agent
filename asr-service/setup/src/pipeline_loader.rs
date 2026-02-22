use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use asr_application::{
    PipelineDefinition, PipelineEngine, PipelineStepLoader, PipelineStepSpec,
};
use asr_configuration::{AppConfig, PipelineDefinitionConfig, PipelineStepRef};
use asr_domain::{DomainError, PipelineStage};
use asr_infra_audio::{AudioPreprocessStage, ResampleStage};
#[cfg(feature = "whisper-runtime")]
use asr_domain::TranscriptionPort;
#[cfg(feature = "whisper-runtime")]
use asr_infra_asr_whisper::{
    WhisperAdapterConfig, WhisperTranscriptionAdapter, WhisperTranscriptionStage,
};
#[cfg(feature = "wav2vec2-runtime")]
use asr_infra_alignment::{Wav2Vec2AdapterConfig, Wav2Vec2AlignmentStage, Wav2Vec2ForcedAligner};

pub trait PipelineStepPlugin: Send + Sync {
    fn name(&self) -> &'static str;
    fn build(&self, config: &AppConfig) -> Result<Arc<dyn PipelineStage>>;
}

pub struct PipelinePluginLoader {
    config: AppConfig,
    plugins: HashMap<String, Arc<dyn PipelineStepPlugin>>,
}

impl PipelinePluginLoader {
    pub fn new(config: AppConfig) -> Self {
        let mut loader = Self {
            config,
            plugins: HashMap::new(),
        };
        loader.register_builtin_plugins();
        loader
    }

    pub fn register_plugin(&mut self, plugin: Arc<dyn PipelineStepPlugin>) {
        self.plugins.insert(plugin.name().to_string(), plugin);
    }

    pub fn build_engine(&self) -> Result<PipelineEngine> {
        let definition = resolve_pipeline_definition(&self.config)?;
        let runtime_definition = to_runtime_definition(&definition)?;
        PipelineEngine::from_definition(&runtime_definition, self)
            .map_err(|err| anyhow!("failed to build pipeline engine: {err}"))
    }

    fn register_builtin_plugins(&mut self) {
        self.register_plugin(Arc::new(AudioClampPlugin));
        self.register_plugin(Arc::new(ResamplePlugin));
        #[cfg(feature = "whisper-runtime")]
        self.register_plugin(Arc::new(WhisperTranscriptionPlugin));
        #[cfg(feature = "wav2vec2-runtime")]
        self.register_plugin(Arc::new(Wav2Vec2AlignmentPlugin));
    }
}

impl PipelineStepLoader for PipelinePluginLoader {
    fn load_step(&self, step: &PipelineStepSpec) -> Result<Arc<dyn PipelineStage>, DomainError> {
        let plugin = self.plugins.get(step.name.as_str()).ok_or_else(|| {
            DomainError::internal_error(&format!("unknown pipeline step plugin `{}`", step.name))
        })?;

        plugin.build(&self.config).map_err(|err| {
            DomainError::internal_error(&format!(
                "failed to build pipeline step `{}`: {err}",
                step.name
            ))
        })
    }
}

fn resolve_pipeline_definition(config: &AppConfig) -> Result<PipelineDefinitionConfig> {
    if let Some(pipeline) = &config.service.pipeline {
        let selected = pipeline.selected.trim();
        if selected.is_empty() {
            return Err(anyhow!("`service.pipeline.selected` cannot be empty"));
        }
        return pipeline.definitions.get(selected).cloned().ok_or_else(|| {
            anyhow!(
                "pipeline `{selected}` not found in `service.pipeline.definitions`"
            )
        });
    }

    Ok(legacy_default_pipeline(config))
}

fn legacy_default_pipeline(config: &AppConfig) -> PipelineDefinitionConfig {
    let mut definition = PipelineDefinitionConfig::default();
    if !config.service.alignment.enabled {
        definition
            .post
            .retain(|step| !step.name().eq_ignore_ascii_case("wav2vec2_alignment"));
    }
    definition
}

fn to_runtime_definition(config: &PipelineDefinitionConfig) -> Result<PipelineDefinition> {
    let pre = config
        .pre
        .iter()
        .map(to_step_spec)
        .collect::<Result<Vec<_>>>()?;
    let transcription = to_step_spec(&config.transcription)?;
    let post = config
        .post
        .iter()
        .map(to_step_spec)
        .collect::<Result<Vec<_>>>()?;

    Ok(PipelineDefinition {
        pre,
        transcription,
        post,
    })
}

fn to_step_spec(step: &PipelineStepRef) -> Result<PipelineStepSpec> {
    let name = step.name().trim();
    if name.is_empty() {
        return Err(anyhow!("pipeline step name cannot be empty"));
    }
    Ok(PipelineStepSpec::new(name.to_string()))
}

#[cfg(feature = "whisper-runtime")]
fn normalize_dtw_mem_size(raw: usize) -> usize {
    const ONE_MIB: usize = 1024 * 1024;
    if raw < ONE_MIB {
        raw.saturating_mul(ONE_MIB)
    } else {
        raw
    }
}

struct AudioClampPlugin;

impl PipelineStepPlugin for AudioClampPlugin {
    fn name(&self) -> &'static str {
        "audio_clamp"
    }

    fn build(&self, _config: &AppConfig) -> Result<Arc<dyn PipelineStage>> {
        Ok(Arc::new(AudioPreprocessStage::new()))
    }
}

struct ResamplePlugin;

impl PipelineStepPlugin for ResamplePlugin {
    fn name(&self) -> &'static str {
        "resample"
    }

    fn build(&self, config: &AppConfig) -> Result<Arc<dyn PipelineStage>> {
        let pipeline_config = config
            .service
            .pipeline
            .as_ref()
            .ok_or_else(|| anyhow!("`resample` step requires `service.pipeline` configuration"))?;

        let resample = &pipeline_config.plugins.resample;
        if !resample.enabled {
            return Err(anyhow!(
                "`resample` step is disabled; set `service.pipeline.plugins.resample.enabled = true`"
            ));
        }

        Ok(Arc::new(ResampleStage::new(resample.target_sample_rate_hz)))
    }
}

#[cfg(feature = "whisper-runtime")]
struct WhisperTranscriptionPlugin;

#[cfg(feature = "whisper-runtime")]
impl PipelineStepPlugin for WhisperTranscriptionPlugin {
    fn name(&self) -> &'static str {
        "whisper_transcription"
    }

    fn build(&self, config: &AppConfig) -> Result<Arc<dyn PipelineStage>> {
        let adapter: Box<dyn TranscriptionPort> = Box::new(WhisperTranscriptionAdapter::new(
            WhisperAdapterConfig {
                model_path: config.service.asr.model_path.clone(),
                language: config.service.asr.default_language.clone(),
                temperature: config.service.asr.temperature,
                threads: config.service.asr.threads,
                dtw_preset: config.service.asr.dtw_preset.clone(),
                dtw_mem_size: normalize_dtw_mem_size(config.service.asr.dtw_mem_size),
            },
        ));
        Ok(Arc::new(WhisperTranscriptionStage::new(adapter)))
    }
}

#[cfg(feature = "wav2vec2-runtime")]
struct Wav2Vec2AlignmentPlugin;

#[cfg(feature = "wav2vec2-runtime")]
impl PipelineStepPlugin for Wav2Vec2AlignmentPlugin {
    fn name(&self) -> &'static str {
        "wav2vec2_alignment"
    }

    fn build(&self, config: &AppConfig) -> Result<Arc<dyn PipelineStage>> {
        let pipeline_cfg = config
            .service
            .pipeline
            .as_ref()
            .ok_or_else(|| {
                anyhow!("`wav2vec2_alignment` requires `service.pipeline` configuration")
            })?;

        let w2v_cfg = &pipeline_cfg.plugins.wav2vec2;
        let adapter_cfg = Wav2Vec2AdapterConfig {
            model_path: w2v_cfg.model_path.clone(),
            config_path: w2v_cfg.config_path.clone(),
            vocab_path: w2v_cfg.vocab_path.clone(),
            device: w2v_cfg.device.clone(),
        };

        let aligner = Wav2Vec2ForcedAligner::load(&adapter_cfg)
            .map_err(|e| anyhow!("wav2vec2 model loading failed: {e}"))?;

        Ok(Arc::new(Wav2Vec2AlignmentStage::new(aligner)))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use asr_configuration::{
        PipelineConfig, PipelineDefinitionConfig, PipelinePluginsConfig, PipelineStepRef,
        ResamplePluginConfig,
    };

    use super::{resolve_pipeline_definition, PipelinePluginLoader};

    #[test]
    fn legacy_pipeline_respects_alignment_toggle() {
        let mut config = asr_configuration::AppConfig::default();
        config.service.pipeline = None;
        config.service.alignment.enabled = false;

        let definition = resolve_pipeline_definition(&config).expect("definition");
        assert!(
            definition
                .post
                .iter()
                .all(|step| !step.name().eq_ignore_ascii_case("wav2vec2_alignment"))
        );
    }

    #[test]
    fn loader_fails_on_unknown_plugin_name() {
        let mut config = asr_configuration::AppConfig::default();
        config.service.pipeline = Some(PipelineConfig {
            selected: "custom".to_string(),
            definitions: HashMap::from([(
                "custom".to_string(),
                PipelineDefinitionConfig {
                    pre: vec![PipelineStepRef::Name("unknown_step".to_string())],
                    transcription: PipelineStepRef::Name("whisper_transcription".to_string()),
                    post: vec![],
                },
            )]),
            plugins: PipelinePluginsConfig::default(),
        });

        let loader = PipelinePluginLoader::new(config);
        let err = match loader.build_engine() {
            Ok(_) => panic!("should fail for unknown plugin"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("unknown pipeline step plugin"));
    }

    #[test]
    fn resample_plugin_requires_enable_flag() {
        let mut config = asr_configuration::AppConfig::default();
        config.service.pipeline = Some(PipelineConfig {
            selected: "custom".to_string(),
            definitions: HashMap::from([(
                "custom".to_string(),
                PipelineDefinitionConfig {
                    pre: vec![PipelineStepRef::Name("resample".to_string())],
                    transcription: PipelineStepRef::Name("whisper_transcription".to_string()),
                    post: vec![],
                },
            )]),
            plugins: PipelinePluginsConfig {
                resample: ResamplePluginConfig {
                    enabled: false,
                    target_sample_rate_hz: 16_000,
                },
                ..Default::default()
            },
        });

        let loader = PipelinePluginLoader::new(config);
        let err = match loader.build_engine() {
            Ok(_) => panic!("should fail when resample is disabled"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("resample` step is disabled"));
    }
}
