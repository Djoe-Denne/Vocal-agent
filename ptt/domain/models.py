"""
Domain models for PTT.
"""

from dataclasses import dataclass
from typing import List

import numpy as np


@dataclass
class AudioChunk:
    """Represents a chunk of audio data for transcription."""

    data: np.ndarray
    sample_rate: int
    chunk_index: int
    timestamp: float
    is_final: bool = False


@dataclass
class TranscriptionResult:
    """Result of a transcription operation."""

    text: str
    segments: List[dict]
    duration: float
    audio_duration: float


@dataclass
class ReconciliationResult:
    """Result of reconciling two text segments."""

    new_text: str
    overlap_found: bool
    overlap_length: int
    confidence: float
