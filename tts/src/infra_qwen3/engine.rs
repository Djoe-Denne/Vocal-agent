//! Qwen3-TTS engine adapter.
//!
//! Implements [`TtsEnginePort`] using the `qwen3-tts-rs` crate (candle-based).
//! Dispatches between preset speaker synthesis ([`engine_speaker`]) and
//! voice clone synthesis ([`engine_clone`]) transparently — from the
//! caller's perspective, `--voice ryan` and `--voice justamon` work
//! the same way.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use qwen3_tts::{AudioBuffer, Qwen3TTS};

use crate::application::config::EngineConfig;
use crate::domain::models::{
    ModelVariant, ResolvedModel, SynthesisRequest, SynthesisResult, SynthesisTiming,
};
use crate::domain::ports::TtsEnginePort;
use crate::domain::value_objects::{SampleRate, VoiceId};

use super::engine_clone::{self, VoiceProfile};
use super::engine_speaker;
use super::mapping::{map_language, map_synthesis_options};

/// Concrete TTS engine backed by `qwen3-tts-rs`.
///
/// Manages model loading, device selection, and synthesis dispatch.
/// Voices are resolved transparently:
///
/// - **Preset speakers** (Ryan, Serena, ...) → `engine_speaker` path
/// - **Custom voices** (any name) → `engine_clone` path, loaded from
///   `voices_dir/<name>/reference.wav`
pub struct Qwen3TtsEngine {
    config: EngineConfig,
    /// Cached model instance (loaded lazily on first synthesis).
    model: Option<LoadedModel>,
}

/// A loaded qwen3-tts model with its source path for cache-key purposes.
struct LoadedModel {
    /// The root directory this model was loaded from.
    root_dir: PathBuf,
    /// The actual qwen3-tts model instance.
    tts: Qwen3TTS,
}

impl Qwen3TtsEngine {
    /// Create a new engine with the given configuration.
    pub fn new(config: EngineConfig) -> Self {
        Self {
            config,
            model: None,
        }
    }

    /// Ensure the model is loaded, reloading if the root_dir changed.
    ///
    /// Always uses `from_pretrained()` with a local directory. HF-downloaded
    /// models are pre-assembled into the expected directory layout by the
    /// HuggingFace provider.
    fn ensure_loaded(
        &mut self,
        resolved: &ResolvedModel,
    ) -> anyhow::Result<&mut Qwen3TTS> {
        let needs_load = match &self.model {
            Some(loaded) => loaded.root_dir != resolved.root_dir,
            None => true,
        };

        if needs_load {
            let device = self.resolve_device()?;
            let root = resolved.root_dir.to_str().ok_or_else(|| {
                anyhow::anyhow!(
                    "Model path contains invalid UTF-8: {:?}",
                    resolved.root_dir
                )
            })?;

            println!("Loading Qwen3-TTS model from {root}...");
            let start = Instant::now();
            let tts = Qwen3TTS::from_pretrained(root, device)?;
            println!(
                "Model loaded in {:.1}s",
                start.elapsed().as_secs_f64()
            );

            self.model = Some(LoadedModel {
                root_dir: resolved.root_dir.clone(),
                tts,
            });
        }

        Ok(&mut self.model.as_mut().unwrap().tts)
    }

    /// Resolve the compute device from config.
    fn resolve_device(&self) -> anyhow::Result<candle_core::Device> {
        match self.config.device.as_str() {
            "auto" => qwen3_tts::auto_device(),
            "cpu" => Ok(candle_core::Device::Cpu),
            s if s.starts_with("cuda") => {
                let ordinal: usize = s
                    .strip_prefix("cuda:")
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);
                Ok(candle_core::Device::new_cuda(ordinal)?)
            }
            #[cfg(feature = "metal")]
            "metal" => Ok(candle_core::Device::new_metal(0)?),
            other => anyhow::bail!("Unknown device: {other:?}"),
        }
    }

    /// Resolve the voice — either a preset speaker or a cloned voice
    /// from the voices directory.
    fn resolve_voice(
        &self,
        voice: &VoiceId,
    ) -> anyhow::Result<ResolvedVoice> {
        if engine_speaker::is_preset_speaker(voice) {
            Ok(ResolvedVoice::Preset)
        } else if let VoiceId::Custom(name) = voice {
            let profile = VoiceProfile::resolve(&self.config.voices_dir, name)?;
            Ok(ResolvedVoice::Clone(profile))
        } else {
            unreachable!()
        }
    }
}

/// Internal resolution of a voice to either preset or clone.
enum ResolvedVoice {
    /// Built-in preset speaker — handled by `engine_speaker`.
    Preset,
    /// Cloned voice from the voices directory — handled by `engine_clone`.
    Clone(VoiceProfile),
}

impl TtsEnginePort for Qwen3TtsEngine {
    fn synthesize(
        &mut self,
        model: &ResolvedModel,
        request: &SynthesisRequest,
    ) -> anyhow::Result<SynthesisResult> {
        // Resolve voice before borrowing self mutably for model loading.
        let resolved_voice = self.resolve_voice(&request.voice)?;

        let tts = self.ensure_loaded(model)?;

        // Dispatch based on model variant and voice type.
        let audio: AudioBuffer = match model.variant {
            ModelVariant::CustomVoice => {
                match resolved_voice {
                    ResolvedVoice::Preset => {
                        engine_speaker::synthesize_speaker(
                            tts,
                            &request.text,
                            &request.voice,
                            request.language,
                            &request.options,
                        )?
                    }
                    ResolvedVoice::Clone(_profile) => {
                        // CustomVoice models don't support cloning.
                        // Warn and fall back to default preset.
                        eprintln!(
                            "Warning: Custom voice on CustomVoice model — \
                             falling back to default speaker (Ryan)."
                        );
                        engine_speaker::synthesize_speaker(
                            tts,
                            &request.text,
                            &VoiceId::Ryan,
                            request.language,
                            &request.options,
                        )?
                    }
                }
            }
            ModelVariant::VoiceDesign => {
                let lang = map_language(request.language);
                let opts = map_synthesis_options(&request.options);
                let instruct = request.instruct.as_deref().unwrap_or(
                    "A natural, clear voice with moderate pace",
                );
                tts.synthesize_voice_design(
                    &request.text,
                    instruct,
                    lang,
                    Some(opts),
                )?
            }
            ModelVariant::Base => {
                match resolved_voice {
                    ResolvedVoice::Clone(profile) => {
                        // Clone from voice profile directory.
                        engine_clone::synthesize_clone(
                            tts,
                            &request.text,
                            &profile,
                            request.language,
                            &request.options,
                        )?
                    }
                    ResolvedVoice::Preset => {
                        // Preset voices on Base models: use explicit ref
                        // audio if provided, otherwise fall back to default.
                        if let Some(ref_audio_path) = &request.ref_audio_path {
                            let ref_audio = AudioBuffer::load(ref_audio_path)?;
                            let prompt = tts.create_voice_clone_prompt(
                                &ref_audio,
                                request.ref_text.as_deref(),
                            )?;
                            let lang = map_language(request.language);
                            let opts = map_synthesis_options(&request.options);
                            tts.synthesize_voice_clone(
                                &request.text,
                                &prompt,
                                lang,
                                Some(opts),
                            )?
                        } else {
                            let opts = map_synthesis_options(&request.options);
                            tts.synthesize(&request.text, Some(opts))?
                        }
                    }
                }
            }
        };

        // Build metadata.
        let mut metadata = HashMap::new();
        metadata.insert("variant".to_owned(), model.variant.to_string());
        metadata.insert("voice".to_owned(), request.voice.to_string());
        metadata.insert("language".to_owned(), request.language.to_string());

        Ok(SynthesisResult {
            audio_samples: audio.samples.clone(),
            sample_rate: SampleRate(audio.sample_rate),
            metadata,
            timings: SynthesisTiming::default(),
            warnings: Vec::new(),
        })
    }
}
