//! Domain → qwen3-tts type mapping.
//!
//! Shared mapping functions used by both the speaker and clone engines.

use crate::domain::value_objects::Language;

/// Map domain `Language` to `qwen3_tts::Language`.
pub fn map_language(lang: Language) -> qwen3_tts::Language {
    match lang {
        Language::English => qwen3_tts::Language::English,
        Language::Chinese => qwen3_tts::Language::Chinese,
        Language::Japanese => qwen3_tts::Language::Japanese,
        Language::Korean => qwen3_tts::Language::Korean,
        Language::German => qwen3_tts::Language::German,
        Language::French => qwen3_tts::Language::French,
        Language::Russian => qwen3_tts::Language::Russian,
        Language::Portuguese => qwen3_tts::Language::Portuguese,
        Language::Spanish => qwen3_tts::Language::Spanish,
        Language::Italian => qwen3_tts::Language::Italian,
    }
}

/// Map domain `SynthesisOptions` to `qwen3_tts::SynthesisOptions`.
pub fn map_synthesis_options(
    opts: &crate::domain::models::SynthesisOptions,
) -> qwen3_tts::SynthesisOptions {
    qwen3_tts::SynthesisOptions {
        temperature: opts.temperature,
        top_k: opts.top_k,
        top_p: opts.top_p,
        repetition_penalty: opts.repetition_penalty,
        seed: opts.seed,
        ..Default::default()
    }
}
