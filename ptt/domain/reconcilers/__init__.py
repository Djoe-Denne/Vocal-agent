"""
PTT Reconcilers (domain layer).
"""

from .word_overlap import WordOverlapReconciler
from .fuzzy import FuzzyReconciler
from .llm import LLMReconciler
from .none import NoOpReconciler
from ptt.domain.reconciler import BaseReconciler, clean_transcription


def create_reconciler(config) -> BaseReconciler:
    algorithm = getattr(config, "reconciler_algorithm", "word_overlap")

    if algorithm == "none":
        return NoOpReconciler()
    if algorithm == "fuzzy":
        return FuzzyReconciler(
            similarity_threshold=getattr(config, "reconciler_fuzzy_threshold", 0.8)
        )
    if algorithm == "llm":
        llm_reconciler = LLMReconciler(
            model_name=getattr(config, "reconciler_llm_model", "HuggingFaceTB/SmolLM2-360M-Instruct"),
            device=getattr(config, "reconciler_llm_device", "cuda"),
            cleanup_hesitations=getattr(config.reconciler, "llm_cleanup_hesitations", True),
        )
        llm_reconciler.set_chain(
            [
                FuzzyReconciler(
                    similarity_threshold=getattr(config, "reconciler_fuzzy_threshold", 0.8),
                    max_context_words=getattr(config, "reconciler_max_context_words", 15),
                )
            ]
        )
        return llm_reconciler

    return WordOverlapReconciler(
        min_overlap_words=getattr(config, "reconciler_min_overlap_words", 3),
        max_context_words=getattr(config, "reconciler_max_context_words", 15),
    )


__all__ = [
    "BaseReconciler",
    "WordOverlapReconciler",
    "FuzzyReconciler",
    "LLMReconciler",
    "NoOpReconciler",
    "create_reconciler",
    "clean_transcription",
]
