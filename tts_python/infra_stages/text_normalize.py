"""
Generic pipeline stage: text normalization.

Performs lightweight text transformations such as whitespace
normalization, optional lowercasing, and abbreviation expansion.
"""

from __future__ import annotations

import re

from shared.application.pipeline_factory import register_stage
from shared.domain.pipeline import MediaType, PipelineContext, Stage

# Simple French abbreviation map — extend as needed
_DEFAULT_ABBREVIATIONS: dict[str, str] = {
    r"\bM\.\b": "Monsieur",
    r"\bMme\b": "Madame",
    r"\bMlle\b": "Mademoiselle",
    r"\bDr\b": "Docteur",
    r"\bSt\b": "Saint",
    r"\bn°\b": "numéro",
}


class TextNormStage(Stage):
    """Normalize text before TTS or further processing (text -> text)."""

    @property
    def name(self) -> str:
        return "text_normalize"

    @property
    def input_type(self) -> MediaType:
        return MediaType.TEXT

    @property
    def output_type(self) -> MediaType:
        return MediaType.TEXT

    def __init__(
        self,
        lowercase: bool = False,
        strip_punctuation: bool = False,
        expand_abbreviations: bool = True,
        **_kwargs,
    ) -> None:
        self._lowercase = lowercase
        self._strip_punctuation = strip_punctuation
        self._expand_abbreviations = expand_abbreviations

    def process(self, ctx: PipelineContext) -> PipelineContext:
        text = ctx.text

        if self._expand_abbreviations:
            for pattern, replacement in _DEFAULT_ABBREVIATIONS.items():
                text = re.sub(pattern, replacement, text)

        if self._lowercase:
            text = text.lower()

        if self._strip_punctuation:
            text = re.sub(r"[^\w\s]", "", text)

        # Normalize whitespace
        text = re.sub(r"\s+", " ", text).strip()

        ctx.text = text
        return ctx


@register_stage("text_normalize")
def _build_text_normalize(cfg: dict) -> Stage:
    return TextNormStage(**cfg)
