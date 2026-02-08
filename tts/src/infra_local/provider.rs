//! Local model provider.
//!
//! Implements [`ModelProviderPort`] for models stored on the local filesystem.
//! Validates directory structure and reads `config.json` to detect the
//! model variant.

use std::collections::HashMap;

use crate::domain::models::{ModelVariant, ResolvedModel};
use crate::domain::ports::ModelProviderPort;
use crate::domain::value_objects::ModelRef;

/// Provides models from local directories.
///
/// Validates that the directory exists and contains the expected files,
/// then reads `config.json` to detect the model variant.
pub struct LocalModelProvider;

impl LocalModelProvider {
    pub fn new() -> Self {
        Self
    }

    /// Detect the model variant by reading `config.json` from the model dir.
    ///
    /// Falls back to heuristic detection from the directory name if
    /// `config.json` is not found or doesn't contain variant info.
    fn detect_variant(model_dir: &std::path::Path) -> anyhow::Result<ModelVariant> {
        let config_path = model_dir.join("config.json");

        if config_path.exists() {
            let contents = std::fs::read_to_string(&config_path)?;
            let json: serde_json::Value = serde_json::from_str(&contents)?;

            // Check for model_type or variant field in config.json.
            if let Some(model_type) = json.get("model_type").and_then(|v| v.as_str()) {
                let lower = model_type.to_lowercase();
                if lower.contains("voicedesign") {
                    return Ok(ModelVariant::VoiceDesign);
                } else if lower.contains("customvoice") {
                    return Ok(ModelVariant::CustomVoice);
                }
            }
        }

        // Heuristic: check directory name.
        let dir_name = model_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();

        if dir_name.contains("voicedesign") {
            Ok(ModelVariant::VoiceDesign)
        } else if dir_name.contains("customvoice") {
            Ok(ModelVariant::CustomVoice)
        } else {
            Ok(ModelVariant::Base)
        }
    }
}

impl Default for LocalModelProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelProviderPort for LocalModelProvider {
    fn prepare(&self, model_ref: &ModelRef) -> anyhow::Result<ResolvedModel> {
        let path = match model_ref {
            ModelRef::Local { path } => path,
            ModelRef::HuggingFace { .. } => {
                anyhow::bail!(
                    "LocalModelProvider received a HuggingFace model ref — \
                     this should have been routed to HuggingFaceModelProvider"
                );
            }
        };

        // Validate directory exists.
        anyhow::ensure!(
            path.exists(),
            "Model directory does not exist: {}",
            path.display()
        );
        anyhow::ensure!(
            path.is_dir(),
            "Model path is not a directory: {}",
            path.display()
        );

        // Check for essential files.
        let has_safetensors = std::fs::read_dir(path)?
            .filter_map(|e| e.ok())
            .any(|e| {
                e.path()
                    .extension()
                    .map_or(false, |ext| ext == "safetensors")
            });

        if !has_safetensors {
            anyhow::bail!(
                "No .safetensors files found in model directory: {}",
                path.display()
            );
        }

        let variant = Self::detect_variant(path)?;

        let mut metadata = HashMap::new();
        metadata.insert("source".to_owned(), "local".to_owned());
        metadata.insert("path".to_owned(), path.display().to_string());

        Ok(ResolvedModel {
            root_dir: path.clone(),
            variant,
            files: None, // Local models use root_dir as a self-contained directory.
            metadata,
        })
    }
}
