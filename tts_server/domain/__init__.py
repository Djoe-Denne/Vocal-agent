"""Domain layer for TTS."""

from .models import SynthesisResult
from .ports import TTSModel

__all__ = ["SynthesisResult", "TTSModel"]
