"""
Domain ports for TTS.
"""

from abc import ABC, abstractmethod
from typing import Optional, Tuple

import numpy as np


class TTSModel(ABC):
    """Abstract port for text-to-speech model adapters."""

    @abstractmethod
    def load(self) -> None:
        raise NotImplementedError

    @abstractmethod
    def unload(self) -> None:
        raise NotImplementedError

    @abstractmethod
    def generate(
        self,
        text: str,
        voice_preset: Optional[str] = None,
        voice_sample_path: Optional[str] = None,
        voice_sample_text: Optional[str] = None,
        guidance: Optional[str] = None,
    ) -> Tuple[np.ndarray, int]:
        """
        Generate speech audio from text.

        Args:
            text: The text to synthesize.
            voice_preset: Optional named speaker preset the model supports natively.
            voice_sample_path: Optional path to a reference audio file for voice cloning.
            voice_sample_text: Optional transcript of the reference audio.
            guidance: Optional free-form instructions to shape the voice output
                      (tone, pacing, emotion, style).

        Returns:
            Tuple of (audio_array, sample_rate).
        """
        raise NotImplementedError

    @abstractmethod
    def get_supported_speakers(self) -> Optional[list[str]]:
        raise NotImplementedError

    @abstractmethod
    def get_supported_languages(self) -> Optional[list[str]]:
        raise NotImplementedError
