"""
TTS application service.

Provides the TTSService orchestrator and a factory for creating adapter instances.
"""

from __future__ import annotations

from pathlib import Path
from typing import Optional, Tuple

import numpy as np

from tts_server.application.config import TTSConfig
from tts_server.domain.models import SynthesisResult
from tts_server.domain.ports import TTSModel


# ---------------------------------------------------------------------------
# Adapter factory
# ---------------------------------------------------------------------------

def create_tts_model(config: TTSConfig) -> TTSModel:
    """
    Factory: select and instantiate the appropriate TTSModel adapter
    based on the configuration.
    """
    from tts_server.infra_pytorch.qwen_model import QwenTTSWrapper

    # Currently only Qwen adapter exists; extend this with elif branches
    # when new adapters are added.
    return QwenTTSWrapper(config.model)


# ---------------------------------------------------------------------------
# Voice-sample resolution helper
# ---------------------------------------------------------------------------

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


# ---------------------------------------------------------------------------
# Application service
# ---------------------------------------------------------------------------

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
        voice_preset: Optional[str] = None,
        guidance: Optional[str] = None,
    ) -> SynthesisResult:
        """
        Synthesize speech from text.

        Either *voice_sample* (a sample-id resolved from the voices directory)
        or *voice_preset* (a model-native speaker name) must be provided.
        An optional *guidance* string can be passed to shape the voice output.
        """
        response_fmt = self._config.output.response_format

        voice_sample_path: Optional[str] = None
        voice_sample_text: Optional[str] = None

        if voice_sample:
            voice_sample_path, voice_sample_text = _resolve_voice_sample(
                self._config.model.voices_dir, voice_sample
            )

        if not voice_sample_path and not voice_preset:
            raise ValueError(
                "Either voice_sample or voice_preset must be provided."
            )

        audio, sr = self._model.generate(
            text=text,
            voice_preset=voice_preset,
            voice_sample_path=voice_sample_path,
            voice_sample_text=voice_sample_text,
            guidance=guidance,
        )

        return SynthesisResult(audio=audio, sample_rate=sr, response_format=response_fmt)

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
