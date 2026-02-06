"""
Domain transcriber interface.
"""

import threading
from abc import ABC, abstractmethod
from typing import Callable, Optional, Any

import numpy as np
import torch

from ptt.application.config import Config
from .models import TranscriptionResult
from ptt.utils.logging import get_logger


class BaseTranscriber(ABC):
    """Abstract base class for ASR transcribers."""

    def __init__(self, config: Config):
        self.config = config
        self._model: Any = None
        self._model_lock = threading.Lock()
        self._log = get_logger("transcriber")

        self._device = "cuda" if torch.cuda.is_available() else "cpu"
        if self._device == "cpu":
            self._log.warning("No GPU detected - using CPU (will be slow!)")

    @property
    def is_loaded(self) -> bool:
        return self._model is not None

    @property
    def device(self) -> str:
        return self._device

    @abstractmethod
    def load_model(self) -> bool:
        raise NotImplementedError

    @abstractmethod
    def unload_model(self) -> bool:
        raise NotImplementedError

    @abstractmethod
    def transcribe_file(self, audio_path) -> Optional[TranscriptionResult]:
        raise NotImplementedError

    @abstractmethod
    def transcribe_array(
        self,
        audio_data: np.ndarray,
        sample_rate: int,
        on_segment: Optional[Callable[[dict], None]] = None,
    ) -> Optional[TranscriptionResult]:
        raise NotImplementedError

    def get_gpu_info(self) -> dict:
        if not torch.cuda.is_available():
            return {}
        try:
            return {
                "name": torch.cuda.get_device_name(0),
                "total_memory_gb": torch.cuda.get_device_properties(0).total_memory / (1024**3),
                "allocated_memory_gb": torch.cuda.memory_allocated() / (1024**3),
                "reserved_memory_gb": torch.cuda.memory_reserved() / (1024**3),
            }
        except Exception:
            return {}

    def _prepare_audio(self, audio_data: np.ndarray, sample_rate: int) -> np.ndarray:
        if audio_data.dtype == np.int16:
            audio_float = audio_data.astype(np.float32) / 32768.0
        elif audio_data.dtype == np.float32:
            audio_float = audio_data
        else:
            audio_float = audio_data.astype(np.float32)

        if len(audio_float.shape) > 1:
            audio_float = audio_float.mean(axis=1)

        if sample_rate != 16000:
            from scipy import signal

            num_samples = int(len(audio_float) * 16000 / sample_rate)
            audio_float = signal.resample(audio_float, num_samples)

        return audio_float
