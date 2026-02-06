"""
Factory functions for creating infrastructure adapters.

The application layer decides which adapter to use based on configuration.
"""

from __future__ import annotations

from ptt.application.config import Config
from ptt.domain.ports import BaseReconciler, BaseTranscriber


def create_transcriber(config: Config) -> BaseTranscriber:
    """Select and instantiate the appropriate ASR transcriber based on config."""
    backend = config.asr.backend.lower()

    if backend == "whisper":
        from ptt.infra_whisper.transcriber import WhisperTranscriber
        return WhisperTranscriber(config)

    if backend == "huggingface":
        from ptt.infra_huggingface.transcriber import HuggingFaceTranscriber
        return HuggingFaceTranscriber(config)

    raise ValueError(f"Unknown ASR backend: {backend!r}. Use 'whisper' or 'huggingface'.")


def create_reconciler(config: Config) -> BaseReconciler:
    """Select and instantiate the appropriate text reconciler based on config."""
    algorithm = config.reconciler.algorithm.lower()

    if algorithm == "none":
        from ptt.infra_reconcilers.none import NoOpReconciler
        return NoOpReconciler()

    if algorithm == "word_overlap":
        from ptt.infra_reconcilers.word_overlap import WordOverlapReconciler
        reconciler = WordOverlapReconciler(
            min_overlap_words=config.reconciler.min_overlap_words,
            max_context_words=config.reconciler.max_context_words,
        )
        # Set fuzzy as a chain fallback
        from ptt.infra_reconcilers.fuzzy import FuzzyReconciler
        fuzzy = FuzzyReconciler(
            similarity_threshold=config.reconciler.fuzzy_threshold,
            max_context_words=config.reconciler.max_context_words,
        )
        reconciler.set_chain([fuzzy])
        return reconciler

    if algorithm == "fuzzy":
        from ptt.infra_reconcilers.fuzzy import FuzzyReconciler
        return FuzzyReconciler(
            similarity_threshold=config.reconciler.fuzzy_threshold,
            max_context_words=config.reconciler.max_context_words,
        )

    if algorithm == "llm":
        from ptt.infra_reconcilers.llm import LLMReconciler
        return LLMReconciler(
            model_name=config.reconciler.llm_model,
            device=config.reconciler.llm_device,
            cleanup_hesitations=config.reconciler.llm_cleanup_hesitations,
        )

    raise ValueError(
        f"Unknown reconciler algorithm: {algorithm!r}. "
        "Use 'none', 'word_overlap', 'fuzzy', or 'llm'."
    )
