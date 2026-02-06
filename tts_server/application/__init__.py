"""Application layer for TTS."""

from .config import TTSConfig, load_config
from .tts_service import TTSService

__all__ = ["TTSConfig", "load_config", "TTSService"]
