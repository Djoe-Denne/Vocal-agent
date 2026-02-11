"""
Domain data models for TTS.
"""

from dataclasses import dataclass

import numpy as np


@dataclass
class SynthesisResult:
    """Result of a TTS synthesis operation."""

    audio: np.ndarray
    sample_rate: int
    response_format: str
