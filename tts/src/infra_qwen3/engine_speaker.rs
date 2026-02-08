//! Preset speaker synthesis.
//!
//! Handles synthesis for built-in preset speakers (CustomVoice models).
//! Maps domain `VoiceId` variants to `qwen3_tts::Speaker` and calls
//! `synthesize_with_voice()`.

use qwen3_tts::{AudioBuffer, Qwen3TTS};

use crate::domain::value_objects::{Language, VoiceId};

use super::mapping::{map_language, map_synthesis_options};

/// Synthesise speech using a preset speaker.
///
/// Only works with CustomVoice models. Preset `VoiceId` variants are
/// mapped 1:1 to `qwen3_tts::Speaker`.
pub fn synthesize_speaker(
    tts: &Qwen3TTS,
    text: &str,
    voice: &VoiceId,
    language: Language,
    options: &crate::domain::models::SynthesisOptions,
) -> anyhow::Result<AudioBuffer> {
    let speaker = map_voice_to_speaker(voice)?;
    let lang = map_language(language);
    let opts = map_synthesis_options(options);

    tts.synthesize_with_voice(text, speaker, lang, Some(opts))
}

/// Map a domain `VoiceId` to a `qwen3_tts::Speaker`.
///
/// Returns an error for `Custom` voices — those must go through
/// the clone path instead.
fn map_voice_to_speaker(voice: &VoiceId) -> anyhow::Result<qwen3_tts::Speaker> {
    match voice {
        VoiceId::Serena => Ok(qwen3_tts::Speaker::Serena),
        VoiceId::Vivian => Ok(qwen3_tts::Speaker::Vivian),
        VoiceId::UncleFu => Ok(qwen3_tts::Speaker::UncleFu),
        VoiceId::Ryan => Ok(qwen3_tts::Speaker::Ryan),
        VoiceId::Aiden => Ok(qwen3_tts::Speaker::Aiden),
        VoiceId::OnoAnna => Ok(qwen3_tts::Speaker::OnoAnna),
        VoiceId::Sohee => Ok(qwen3_tts::Speaker::Sohee),
        VoiceId::Eric => Ok(qwen3_tts::Speaker::Eric),
        VoiceId::Dylan => Ok(qwen3_tts::Speaker::Dylan),
        VoiceId::Custom(name) => {
            anyhow::bail!(
                "Custom voice {name:?} is not a preset speaker. \
                 It should be routed to the voice clone path."
            )
        }
    }
}

/// Check whether a `VoiceId` is a built-in preset speaker.
pub fn is_preset_speaker(voice: &VoiceId) -> bool {
    !matches!(voice, VoiceId::Custom(_))
}
