"""
TTS application service.
"""

from __future__ import annotations

from pathlib import Path
from typing import Optional, Tuple

import numpy as np

from tts_server.application.config import TTSConfig
from tts_server.domain.ports import TTSModel


def _resolve_voice_sample(voices_dir: Path, sample_id: str) -> Tuple[str, Optional[str]]:
    sample_dir = voices_dir / sample_id
    if not sample_dir.exists():
        raise ValueError(f"Voice sample '{sample_id}' not found in {voices_dir}.")

    audio_candidates = ["audio.wav", "audio.flac", "audio.ogg", "audio.mp3"]
    audio_path = None
    for name in audio_candidates:
        candidate = sample_dir / name
        if candidate.exists():
            audio_path = candidate
            break
    if audio_path is None:
        raise ValueError(f"Voice sample '{sample_id}' is missing audio.wav (or flac/ogg/mp3).")

    text_path = sample_dir / "text.txt"
    text_value = None
    if text_path.exists():
        text_value = text_path.read_text(encoding="utf-8").strip()

    return str(audio_path), text_value


class TTSService:
    def __init__(self, config: TTSConfig, model: TTSModel) -> None:
        self._config = config
        self._model = model

    @property
    def config(self) -> TTSConfig:
        return self._config

    def synthesize(
        self,
        text: str,
        voice_sample: Optional[str] = None,
    ) -> Tuple[np.ndarray, int, str]:
        response_fmt = self._config.output.response_format

        if not voice_sample:
            raise ValueError("voice_sample is required for CustomVoice sample-based synthesis.")

        voice_sample_path, voice_sample_text = _resolve_voice_sample(
            self._config.model.voices_dir, voice_sample
        )

        audio, sr = self._model.generate(
            text=text,
            voice_sample_path=voice_sample_path,
            voice_sample_text=voice_sample_text,
        )

        return audio, sr, response_fmt

    def list_voices(self) -> dict:
        return {
            "speakers": self._model.get_supported_speakers(),
            "languages": self._model.get_supported_languages(),
            "voice_samples_dir": str(self._config.model.voices_dir),
            "voice_sample_layout": {
                "example": "voices/my_voice/",
                "audio": ["audio.wav", "audio.flac", "audio.ogg", "audio.mp3"],
                "text": "text.txt (optional)",
            },
        }
