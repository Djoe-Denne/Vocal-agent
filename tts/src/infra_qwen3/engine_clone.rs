//! Voice clone synthesis.
//!
//! Handles synthesis for cloned voices stored in the `voices/` directory.
//! Each voice profile is a subdirectory containing:
//!
//! ```text
//! voices/
//!   <voice_name>/
//!     reference.wav           ← required: reference audio sample
//!     transcript.txt          ← optional: transcript for ICL mode (better quality)
//! ```
//!
//! From the user's perspective, cloned voices are used exactly like preset
//! speakers: `--voice justamon` works the same as `--voice ryan`.

use std::path::{Path, PathBuf};

use qwen3_tts::{AudioBuffer, Qwen3TTS};

use crate::domain::value_objects::Language;

use super::mapping::{map_language, map_synthesis_options};

/// A resolved voice clone profile loaded from the voices directory.
#[derive(Debug)]
pub struct VoiceProfile {
    /// Voice name (directory name).
    pub name: String,
    /// Path to the reference audio file.
    pub reference_audio: PathBuf,
    /// Optional transcript text for ICL mode.
    pub transcript: Option<String>,
}

impl VoiceProfile {
    /// Resolve a voice profile from the voices directory.
    ///
    /// Looks for `<voices_dir>/<voice_name>/reference.wav` and
    /// optionally `<voices_dir>/<voice_name>/transcript.txt`.
    pub fn resolve(voices_dir: &Path, voice_name: &str) -> anyhow::Result<Self> {
        let voice_dir = voices_dir.join(voice_name);

        anyhow::ensure!(
            voice_dir.exists() && voice_dir.is_dir(),
            "Voice profile directory not found: {}\n\
             Create it with a reference.wav file inside.",
            voice_dir.display()
        );

        // Look for reference audio (try common names).
        let reference_audio = find_reference_audio(&voice_dir)?;

        // Optionally load transcript.
        let transcript = load_transcript(&voice_dir);

        Ok(Self {
            name: voice_name.to_owned(),
            reference_audio,
            transcript,
        })
    }

    /// List all available voice profiles in the voices directory.
    pub fn list_available(voices_dir: &Path) -> Vec<String> {
        if !voices_dir.exists() {
            return Vec::new();
        }

        std::fs::read_dir(voices_dir)
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let entry = entry.ok()?;
                if entry.path().is_dir() {
                    // Only list directories that contain a reference audio file.
                    let has_ref = find_reference_audio(&entry.path()).is_ok();
                    if has_ref {
                        entry.file_name().to_str().map(|s| s.to_owned())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect()
    }
}

/// Synthesise speech using a cloned voice profile.
///
/// Loads the reference audio from the voice profile directory,
/// creates a voice clone prompt, and runs synthesis.
pub fn synthesize_clone(
    tts: &Qwen3TTS,
    text: &str,
    profile: &VoiceProfile,
    language: Language,
    options: &crate::domain::models::SynthesisOptions,
) -> anyhow::Result<AudioBuffer> {
    let lang = map_language(language);
    let opts = map_synthesis_options(options);

    // Load reference audio.
    let ref_audio = AudioBuffer::load(&profile.reference_audio)?;

    // Create voice clone prompt (ICL if transcript available, x-vector if not).
    let prompt = tts.create_voice_clone_prompt(
        &ref_audio,
        profile.transcript.as_deref(),
    )?;

    println!(
        "Voice clone: {} ({})",
        profile.name,
        if profile.transcript.is_some() {
            "ICL mode"
        } else {
            "x-vector mode"
        }
    );

    tts.synthesize_voice_clone(text, &prompt, lang, Some(opts))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Find the reference audio file in a voice profile directory.
///
/// Tries common file names: `reference.wav`, `ref.wav`, or any `.wav` file.
fn find_reference_audio(voice_dir: &Path) -> anyhow::Result<PathBuf> {
    // Try well-known names first.
    for name in &["reference.wav", "ref.wav"] {
        let path = voice_dir.join(name);
        if path.exists() {
            return Ok(path);
        }
    }

    // Fall back to any .wav file in the directory.
    if let Ok(entries) = std::fs::read_dir(voice_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .map_or(false, |ext| ext.eq_ignore_ascii_case("wav"))
            {
                return Ok(path);
            }
        }
    }

    anyhow::bail!(
        "No reference audio (.wav) found in voice profile: {}",
        voice_dir.display()
    )
}

/// Load the transcript from a voice profile directory, if present.
fn load_transcript(voice_dir: &Path) -> Option<String> {
    let path = voice_dir.join("transcript.txt");
    if path.exists() {
        std::fs::read_to_string(&path)
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
    } else {
        None
    }
}
