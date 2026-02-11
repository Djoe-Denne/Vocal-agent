"""
Backward-compatibility re-export.  Canonical location: application.config
"""

from tts_python.application.config import (  # noqa: F401
    ServerConfig,
    ModelConfig,
    OutputConfig,
    TTSConfig,
    load_config,
)

__all__ = ["ServerConfig", "ModelConfig", "OutputConfig", "TTSConfig", "load_config"]
