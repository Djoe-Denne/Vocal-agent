"""
PyTorch-backed transcribers (adapter layer).
"""

from ptt.transcriber import (
    BaseTranscriber,
    WhisperTranscriber,
    HuggingFaceTranscriber,
    create_transcriber,
    TranscriptionResult,
)

__all__ = [
    "BaseTranscriber",
    "WhisperTranscriber",
    "HuggingFaceTranscriber",
    "create_transcriber",
    "TranscriptionResult",
]
