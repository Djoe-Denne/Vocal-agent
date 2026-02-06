"""
Qwen3 TTS model wrapper (PyTorch adapter).

Implements the TTSModel domain port using Qwen3-TTS.
Supports voice cloning via reference audio, built-in voice presets,
and guidance instructions to shape the voice output.
"""

from __future__ import annotations

from dataclasses import asdict
from typing import Optional, Tuple

import numpy as np
import torch
from qwen_tts import Qwen3TTSModel

from tts_server.application.config import ModelConfig
from tts_server.domain.ports import TTSModel


_DTYPE_MAP = {
    "float16": torch.float16,
    "bfloat16": torch.bfloat16,
    "float32": torch.float32,
}


class QwenTTSWrapper(TTSModel):
    """Qwen3-TTS adapter implementing the TTSModel port."""

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
        voice_preset: Optional[str] = None,
        voice_sample_path: Optional[str] = None,
        voice_sample_text: Optional[str] = None,
        guidance: Optional[str] = None,
    ) -> Tuple[np.ndarray, int]:
        if not text.strip():
            raise ValueError("Input text is empty.")

        self.load()
        assert self._model is not None

        language = (self._config.language or "English").strip() or "English"

        # Build the effective text, prepending guidance if provided
        effective_text = text
        if guidance:
            effective_text = f"[{guidance}] {text}"

        # Route to voice-cloning or preset-based generation
        if voice_sample_path:
            wavs, sr = self._model.generate_voice_clone(
                text=effective_text,
                language=language,
                ref_audio=voice_sample_path,
                ref_text=voice_sample_text,
            )
        elif voice_preset:
            # Use the model's built-in speaker preset
            wavs, sr = self._model.generate(
                text=effective_text,
                language=language,
                speaker=voice_preset,
            )
        else:
            raise ValueError(
                "Either voice_preset or voice_sample_path must be provided."
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
