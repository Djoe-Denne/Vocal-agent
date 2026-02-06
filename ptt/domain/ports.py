"""
Domain port interfaces for PTT.

Pure abstract contracts that infrastructure adapters must implement.
"""

import re
from abc import ABC, abstractmethod
from pathlib import Path
from typing import Callable, List, Optional

import numpy as np

from .models import ReconciliationResult, TranscriptionResult


# ---------------------------------------------------------------------------
# Transcriber port
# ---------------------------------------------------------------------------

class BaseTranscriber(ABC):
    """Abstract port for ASR transcription adapters."""

    @property
    @abstractmethod
    def is_loaded(self) -> bool:
        """Whether the underlying model is currently loaded."""
        ...

    @abstractmethod
    def load_model(self) -> bool:
        """Load the model into memory. Returns True on success."""
        ...

    @abstractmethod
    def unload_model(self) -> bool:
        """Unload the model from memory. Returns True on success."""
        ...

    @abstractmethod
    def transcribe_file(self, audio_path: Path) -> Optional[TranscriptionResult]:
        """Transcribe an audio file."""
        ...

    @abstractmethod
    def transcribe_array(
        self,
        audio_data: np.ndarray,
        sample_rate: int,
        on_segment: Optional[Callable[[dict], None]] = None,
    ) -> Optional[TranscriptionResult]:
        """Transcribe audio from a numpy array."""
        ...


# ---------------------------------------------------------------------------
# Text-cleaning utility (used by reconcilers)
# ---------------------------------------------------------------------------

def clean_transcription(text: str) -> str:
    """Clean up transcription text by removing common artifacts."""
    if not text:
        return ""

    text = text.strip()

    if len(text) >= 2:
        quote_pairs = [('"', '"'), ("'", "'"), ("\u201c", "\u201d"), ("\u00ab", "\u00bb"), ("\u2018", "\u2019")]
        for left, right in quote_pairs:
            if text.startswith(left) and text.endswith(right):
                text = text[1:-1].strip()
                break

    text = re.sub(r'^[\"\u201c\u201d\u00ab\u00bb\'`]+', '', text)
    text = re.sub(r'[\"\u201c\u201d\u00ab\u00bb\'`]+$', '', text)

    text = re.sub(r'\.{3,}', ' ', text)
    text = re.sub(r'\u2026', ' ', text)

    text = re.sub(r'([!?])\1{1,}', r'\1', text)
    text = re.sub(r'([.,])\1{1,}', r'\1', text)

    text = re.sub(r'\s+', ' ', text)

    return text.strip()


# ---------------------------------------------------------------------------
# Reconciler port
# ---------------------------------------------------------------------------

class BaseReconciler(ABC):
    """
    Abstract port for text reconciliation adapters.

    Concrete subclasses only need to implement ``reconcile()``.
    The segment-management logic (``add_segment``, ``get_full_text``, etc.)
    is shared domain behaviour.
    """

    def __init__(self) -> None:
        self._segments: list[str] = []
        self._full_text: str = ""
        self._chain: list["BaseReconciler"] = []

    @abstractmethod
    def reconcile(self, previous_text: str, current_text: str) -> ReconciliationResult:
        """Find overlap between two text segments."""
        ...

    # -- shared domain logic --------------------------------------------------

    def add_segment(self, text: str) -> ReconciliationResult:
        text = clean_transcription(text)
        if not text:
            return ReconciliationResult(
                new_text="", overlap_found=False, overlap_length=0, confidence=1.0
            )

        if not self._segments:
            self._segments.append(text)
            self._full_text = text
            return ReconciliationResult(
                new_text=text, overlap_found=False, overlap_length=0, confidence=1.0
            )

        previous = self._segments[-1]
        word_result = self._try_word_overlap(previous, text)
        if word_result.overlap_found and not self._chain:
            result = word_result
        else:
            current_text = word_result.new_text if word_result.overlap_found else text
            if self._chain:
                current_text, chain_result = self._apply_chain(previous, current_text)
            else:
                chain_result = None

            if not current_text:
                result = chain_result or ReconciliationResult(
                    new_text="", overlap_found=False, overlap_length=0, confidence=1.0
                )
            else:
                result = self.reconcile(previous, current_text)

        self._segments.append(text)
        if result.new_text:
            result = ReconciliationResult(
                new_text=clean_transcription(result.new_text),
                overlap_found=result.overlap_found,
                overlap_length=result.overlap_length,
                confidence=result.confidence,
            )
            if result.new_text:
                self._full_text += " " + result.new_text

        return result

    def _try_word_overlap(
        self,
        previous_text: str,
        current_text: str,
        min_words: int = 2,
        max_context: int = 15,
    ) -> ReconciliationResult:
        if not previous_text or not current_text:
            return ReconciliationResult(
                new_text=current_text, overlap_found=False, overlap_length=0, confidence=1.0
            )

        prev_words = previous_text.lower().split()
        curr_words = current_text.lower().split()
        curr_words_original = current_text.split()

        max_overlap = min(len(prev_words), len(curr_words), max_context)
        for overlap_len in range(max_overlap, min_words - 1, -1):
            if prev_words[-overlap_len:] == curr_words[:overlap_len]:
                new_words = curr_words_original[overlap_len:]
                return ReconciliationResult(
                    new_text=" ".join(new_words),
                    overlap_found=True,
                    overlap_length=overlap_len,
                    confidence=1.0,
                )

        return ReconciliationResult(
            new_text=current_text, overlap_found=False, overlap_length=0, confidence=0.0
        )

    def _apply_chain(
        self, previous_text: str, current_text: str
    ) -> tuple[str, Optional[ReconciliationResult]]:
        result = None
        for reconciler in self._chain:
            result = reconciler.reconcile(previous_text, current_text)
            if result.overlap_found:
                current_text = result.new_text
        return current_text, result

    def get_full_text(self) -> str:
        return self._full_text.strip()

    def reset(self) -> None:
        self._segments.clear()
        self._full_text = ""

    def set_chain(self, chain: list["BaseReconciler"]) -> None:
        self._chain = chain
