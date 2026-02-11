"""
Generic pipeline stage: regex-based text cleanup.

Removes patterns (hesitations, fillers, etc.) from ctx.text
based on a configurable list of regex patterns.
"""

from __future__ import annotations

import re

from shared.application.pipeline_factory import register_stage
from shared.domain.pipeline import MediaType, PipelineContext, Stage


class RegexCleanupStage(Stage):
    """Remove text matching one or more regex patterns (text -> text)."""

    @property
    def name(self) -> str:
        return "regex_cleanup"

    @property
    def input_type(self) -> MediaType:
        return MediaType.TEXT

    @property
    def output_type(self) -> MediaType:
        return MediaType.TEXT

    def __init__(self, patterns: list[str] | None = None, **_kwargs) -> None:
        raw = patterns or []
        self._compiled = [re.compile(p, re.IGNORECASE) for p in raw]

    def process(self, ctx: PipelineContext) -> PipelineContext:
        text = ctx.text
        for pattern in self._compiled:
            text = pattern.sub("", text)
        # Collapse extra whitespace left behind by removals
        ctx.text = re.sub(r"\s+", " ", text).strip()
        return ctx


@register_stage("regex_cleanup")
def _build_regex_cleanup(cfg: dict) -> Stage:
    return RegexCleanupStage(**cfg)
