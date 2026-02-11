"""
Shared domain: universal pipeline stage contract.

Defines the Stage ABC, PipelineContext dataclass, and MediaType enum
that every pipeline stage — regardless of module — must adhere to.
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from enum import Enum, auto
from typing import Any, Optional


class MediaType(Enum):
    """The kind of payload a stage consumes or produces."""

    AUDIO = auto()  # np.ndarray + sample_rate
    TEXT = auto()  # str


@dataclass
class PipelineContext:
    """
    Mutable bag of data that flows through every stage in a pipeline.

    Stages read the fields they need and write back their results.
    """

    # Primary payload
    audio: Optional[Any] = None  # np.ndarray
    sample_rate: int = 16000
    text: str = ""

    # Metadata bag — stages can stash anything here
    meta: dict[str, Any] = field(default_factory=dict)

    # Accumulated diagnostics / timing per stage
    stage_results: list[dict] = field(default_factory=list)


class Stage(ABC):
    """
    Single processing step in a pipeline.

    Every pre-processor, model, and post-processor implements this.
    """

    @property
    @abstractmethod
    def name(self) -> str:
        """Unique identifier used in config (e.g. 'vad', 'whisper', 'cleanup')."""
        ...

    @property
    def input_type(self) -> MediaType:
        """What this stage consumes.  Default: TEXT."""
        return MediaType.TEXT

    @property
    def output_type(self) -> MediaType:
        """What this stage produces.  Default: same as input."""
        return self.input_type

    @abstractmethod
    def process(self, ctx: PipelineContext) -> PipelineContext:
        """Transform the context in-place and return it."""
        ...

    def load(self) -> None:
        """Optional: load heavy resources (models, etc.)."""

    def unload(self) -> None:
        """Optional: free resources."""


__all__ = [
    "MediaType",
    "PipelineContext",
    "Stage",
]
