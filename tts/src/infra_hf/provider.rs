//! HuggingFace model provider.
//!
//! Implements [`ModelProviderPort`] by downloading models from the
//! HuggingFace Hub and assembling a local directory that
//! `Qwen3TTS::from_pretrained()` can load directly (with proper
//! `config.json` parsing for all model variants).

use std::collections::HashMap;
use std::path::PathBuf;

use crate::domain::models::{ModelVariant, ResolvedModel};
use crate::domain::ports::ModelProviderPort;
use crate::domain::value_objects::ModelRef;

/// Downloads and caches models from the HuggingFace Hub.
///
/// After downloading, assembles a local staging directory in `cache_dir`
/// with the layout that `Qwen3TTS::from_pretrained()` expects:
///
/// ```text
/// cache_dir/<safe_repo_name>/
///   config.json
///   model.safetensors          ← hard-linked from HF cache
///   speech_tokenizer/
///     model.safetensors        ← hard-linked from HF cache
///   tokenizer.json             ← hard-linked from HF cache
/// ```
///
/// This ensures `from_pretrained` can read `config.json` and correctly
/// detect model dimensions (critical for 1.7B models that need a
/// projection layer in the code predictor).
pub struct HuggingFaceModelProvider {
    /// Local cache directory for staging model directories.
    cache_dir: PathBuf,
}

impl HuggingFaceModelProvider {
    /// Create a provider with the given cache directory.
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    /// Detect the model variant from the HuggingFace repo name.
    fn detect_variant_from_repo(repo: &str) -> ModelVariant {
        let lower = repo.to_lowercase();
        if lower.contains("voicedesign") {
            ModelVariant::VoiceDesign
        } else if lower.contains("customvoice") {
            ModelVariant::CustomVoice
        } else {
            ModelVariant::Base
        }
    }

    /// Convert a HF repo name to a safe directory name.
    ///
    /// e.g. `"Qwen/Qwen3-TTS-12Hz-1.7B-Base"` → `"Qwen--Qwen3-TTS-12Hz-1.7B-Base"`
    fn safe_dir_name(repo: &str) -> String {
        repo.replace('/', "--")
    }

    /// Assemble a local model directory from downloaded HF cache files.
    ///
    /// Uses hard links when possible (no extra disk space), falls back
    /// to copy if hard link fails (e.g. cross-volume).
    fn assemble_local_dir(
        staging_dir: &std::path::Path,
        paths: &qwen3_tts::ModelPaths,
    ) -> anyhow::Result<()> {
        std::fs::create_dir_all(staging_dir)?;

        // config.json — small, always copy.
        let config_target = staging_dir.join("config.json");
        if !config_target.exists() && paths.config.exists() {
            std::fs::copy(&paths.config, &config_target)?;
        }

        // model.safetensors — large, hard-link or copy.
        let model_target = staging_dir.join("model.safetensors");
        if !model_target.exists() {
            link_or_copy(&paths.model_weights, &model_target)?;
        }

        // speech_tokenizer/model.safetensors — large, hard-link or copy.
        let st_dir = staging_dir.join("speech_tokenizer");
        std::fs::create_dir_all(&st_dir)?;
        let st_target = st_dir.join("model.safetensors");
        if !st_target.exists() {
            link_or_copy(&paths.decoder_weights, &st_target)?;
        }

        // tokenizer.json — small-ish, hard-link or copy.
        let tok_target = staging_dir.join("tokenizer.json");
        if !tok_target.exists() {
            link_or_copy(&paths.tokenizer, &tok_target)?;
        }

        Ok(())
    }
}

/// Hard-link a file, falling back to copy if hard link fails.
fn link_or_copy(src: &std::path::Path, dst: &std::path::Path) -> anyhow::Result<()> {
    std::fs::hard_link(src, dst).or_else(|_| {
        std::fs::copy(src, dst).map(|_| ())
    })?;
    Ok(())
}

impl ModelProviderPort for HuggingFaceModelProvider {
    fn prepare(&self, model_ref: &ModelRef) -> anyhow::Result<ResolvedModel> {
        let (repo, _revision) = match model_ref {
            ModelRef::HuggingFace { repo, revision } => (repo, revision),
            ModelRef::Local { .. } => {
                anyhow::bail!(
                    "HuggingFaceModelProvider received a Local model ref — \
                     this should have been routed to LocalModelProvider"
                );
            }
        };

        println!("Downloading model from HuggingFace: {repo}...");

        // Download model files to the HuggingFace cache.
        let paths = qwen3_tts::ModelPaths::download(Some(repo.as_str()))?;

        // Assemble a local directory with the layout from_pretrained expects.
        let staging_dir = self.cache_dir.join(Self::safe_dir_name(repo));
        Self::assemble_local_dir(&staging_dir, &paths)?;

        let variant = Self::detect_variant_from_repo(repo);

        let mut metadata = HashMap::new();
        metadata.insert("source".to_owned(), "huggingface".to_owned());
        metadata.insert("repo".to_owned(), repo.clone());

        Ok(ResolvedModel {
            root_dir: staging_dir,
            variant,
            files: None, // Engine uses from_pretrained with staging dir.
            metadata,
        })
    }
}
