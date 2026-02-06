"""
Domain ports for TTS.
"""

from abc import ABC, abstractmethod
from typing import Optional, Tuple

import numpy as np


class TTSModel(ABC):
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
        voice_sample_path: Optional[str] = None,
        voice_sample_text: Optional[str] = None,
    ) -> Tuple[np.ndarray, int]:
        raise NotImplementedError

    @abstractmethod
    def get_supported_speakers(self) -> Optional[list[str]]:
        raise NotImplementedError

    @abstractmethod
    def get_supported_languages(self) -> Optional[list[str]]:
        raise NotImplementedError
