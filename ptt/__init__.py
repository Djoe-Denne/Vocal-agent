"""
PTT - Push-to-Talk Speech-to-Text for Windows

A modular speech-to-text application with push-to-talk functionality,
real-time streaming transcription, and OpenClaw AI integration.
"""

__version__ = "2.0.0"
__author__ = "PTT Project"

__all__ = [
    "Config",
    "ASRConfig",
    "WhisperConfig",
    "HuggingFaceConfig",
    "load_config",
    "BaseTranscriber",
    "WhisperTranscriber",
    "HuggingFaceTranscriber",
    "create_transcriber",
    "Transcriber",  # Backward compatibility
    "StreamingRecorder",
    "OpenClawClient",
    "HotkeyManager",
]


def __getattr__(name: str):
    if name in {"Config", "ASRConfig", "WhisperConfig", "HuggingFaceConfig", "load_config"}:
        from .config import Config, ASRConfig, WhisperConfig, HuggingFaceConfig, load_config

        return {
            "Config": Config,
            "ASRConfig": ASRConfig,
            "WhisperConfig": WhisperConfig,
            "HuggingFaceConfig": HuggingFaceConfig,
            "load_config": load_config,
        }[name]
    if name in {
        "BaseTranscriber",
        "WhisperTranscriber",
        "HuggingFaceTranscriber",
        "create_transcriber",
        "Transcriber",
    }:
        from .transcriber import (
            BaseTranscriber,
            WhisperTranscriber,
            HuggingFaceTranscriber,
            create_transcriber,
            Transcriber,
        )

        return {
            "BaseTranscriber": BaseTranscriber,
            "WhisperTranscriber": WhisperTranscriber,
            "HuggingFaceTranscriber": HuggingFaceTranscriber,
            "create_transcriber": create_transcriber,
            "Transcriber": Transcriber,
        }[name]
    if name == "StreamingRecorder":
        from .recorder import StreamingRecorder

        return StreamingRecorder
    if name == "OpenClawClient":
        from .openclaw import OpenClawClient

        return OpenClawClient
    if name == "HotkeyManager":
        from .hotkeys import HotkeyManager

        return HotkeyManager
    raise AttributeError(f"module 'ptt' has no attribute {name!r}")
