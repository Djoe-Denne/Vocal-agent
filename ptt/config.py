"""
Backward-compatibility re-export.  Canonical location: application.config
"""

from ptt.application.config import (  # noqa: F401
    WhisperConfig,
    HuggingFaceConfig,
    ASRConfig,
    ReconcilerConfig,
    StreamingConfig,
    DaemonConfig,
    ApiConfig,
    Config,
    load_config,
)

__all__ = [
    "WhisperConfig",
    "HuggingFaceConfig",
    "ASRConfig",
    "ReconcilerConfig",
    "StreamingConfig",
    "DaemonConfig",
    "ApiConfig",
    "Config",
    "load_config",
]
