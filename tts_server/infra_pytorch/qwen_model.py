"""
Qwen3 TTS model wrapper (PyTorch adapter).

Uses the Base model's generate_voice_clone for sample-based voice cloning.
"""

from __future__ import annotations

from dataclasses import asdict
from typing import Callable, Optional, Tuple

import numpy as np
import torch
from qwen_tts import Qwen3TTSModel

from tts_server.config import ModelConfig


_DTYPE_MAP = {
    "float16": torch.float16,
    "bfloat16": torch.bfloat16,
    "float32": torch.float32,
}


class QwenTTSWrapper:
    def __init__(self, config: ModelConfig) -> None:
        self._config = config
        self._model: Optional[Qwen3TTSModel] = None

    @property
    def config(self) -> ModelConfig:
        return self._config

    def load(self) -> None:
        if self._model is not None:
            return

        dtype = _DTYPE_MAP.get(self._config.torch_dtype, torch.bfloat16)
        kwargs = {
            "device_map": self._config.device_map,
            "dtype": dtype,
        }

        self._model = Qwen3TTSModel.from_pretrained(self._config.model, **kwargs)

    def unload(self) -> None:
        if self._model is None:
            return
        self._model = None
        if torch.cuda.is_available():
            torch.cuda.empty_cache()

    def generate(
        self,
        text: str,
        voice_sample_path: Optional[str] = None,
        voice_sample_text: Optional[str] = None,
    ) -> Tuple[np.ndarray, int]:
        if not text.strip():
            raise ValueError("Input text is empty.")

        if not voice_sample_path:
            raise ValueError("voice_sample is required for sample-based voice cloning.")

        self.load()
        assert self._model is not None

        language = (self._config.language or "English").strip() or "English"

        wavs, sr = self._model.generate_voice_clone(
            text=text,
            language=language,
            ref_audio=voice_sample_path,
            ref_text=voice_sample_text,
        )

        if not wavs:
            raise RuntimeError("Model did not return audio.")
        audio = wavs[0]
        if isinstance(audio, torch.Tensor):
            audio = audio.detach().cpu().numpy()
        return np.asarray(audio), int(sr)

    def get_supported_speakers(self) -> Optional[list[str]]:
        self.load()
        assert self._model is not None
        speakers = getattr(self._model.model, "get_supported_speakers", None)
        if callable(speakers):
            values = speakers()
            if values is None:
                return None
            return [str(s) for s in values]
        return None

    def get_supported_languages(self) -> Optional[list[str]]:
        self.load()
        assert self._model is not None
        langs = getattr(self._model.model, "get_supported_languages", None)
        if callable(langs):
            values = langs()
            if values is None:
                return None
            return [str(s) for s in values]
        return None

    def as_dict(self) -> dict:
        return asdict(self._config)
