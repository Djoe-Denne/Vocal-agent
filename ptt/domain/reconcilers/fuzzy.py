"""
PTT Fuzzy Reconciler (domain).
"""

from ptt.domain.reconciler import BaseReconciler
from ptt.domain.models import ReconciliationResult


class FuzzyReconciler(BaseReconciler):
    def __init__(self, similarity_threshold: float = 0.8, max_context_words: int = 15):
        super().__init__()
        self.similarity_threshold = similarity_threshold
        self.max_context_words = max_context_words
        self._rapidfuzz_available = None

    def _check_rapidfuzz(self) -> bool:
        if self._rapidfuzz_available is None:
            try:
                import rapidfuzz  # noqa: F401

                self._rapidfuzz_available = True
            except ImportError:
                self._rapidfuzz_available = False
        return self._rapidfuzz_available

    def reconcile(self, previous_text: str, current_text: str) -> ReconciliationResult:
        if not previous_text or not current_text:
            return ReconciliationResult(
                new_text=current_text,
                overlap_found=False,
                overlap_length=0,
                confidence=1.0,
            )

        if not self._check_rapidfuzz():
            return self._simple_fallback(previous_text, current_text)

        from rapidfuzz import fuzz

        prev_words = previous_text.split()
        curr_words = current_text.split()
        curr_words_original = current_text.split()

        if not prev_words or not curr_words:
            return ReconciliationResult(
                new_text=current_text,
                overlap_found=False,
                overlap_length=0,
                confidence=1.0,
            )

        context_words = prev_words[-self.max_context_words:]
        best_overlap_len = 0
        best_confidence = 0.0
        max_possible = min(len(context_words), len(curr_words))

        for overlap_len in range(3, max_possible + 1):
            prev_phrase = " ".join(context_words[-overlap_len:])
            curr_phrase = " ".join(curr_words[:overlap_len])
            similarity = fuzz.ratio(prev_phrase.lower(), curr_phrase.lower()) / 100.0
            if similarity >= self.similarity_threshold and similarity > best_confidence:
                best_overlap_len = overlap_len
                best_confidence = similarity

        if best_overlap_len > 0:
            new_words = curr_words_original[best_overlap_len:]
            new_text = " ".join(new_words)
            return ReconciliationResult(
                new_text=new_text,
                overlap_found=True,
                overlap_length=best_overlap_len,
                confidence=best_confidence,
            )

        return ReconciliationResult(
            new_text=current_text,
            overlap_found=False,
            overlap_length=0,
            confidence=1.0,
        )

    def _simple_fallback(self, previous_text: str, current_text: str) -> ReconciliationResult:
        prev_words = previous_text.lower().split()
        curr_words = current_text.lower().split()
        curr_words_original = current_text.split()

        if not prev_words or not curr_words:
            return ReconciliationResult(
                new_text=current_text,
                overlap_found=False,
                overlap_length=0,
                confidence=1.0,
            )

        context_words = prev_words[-self.max_context_words:]
        for overlap_len in range(min(len(context_words), len(curr_words)), 2, -1):
            prev_end = context_words[-overlap_len:]
            curr_start = curr_words[:overlap_len]
            if prev_end == curr_start:
                new_words = curr_words_original[overlap_len:]
                return ReconciliationResult(
                    new_text=" ".join(new_words),
                    overlap_found=True,
                    overlap_length=overlap_len,
                    confidence=1.0,
                )

        return ReconciliationResult(
            new_text=current_text,
            overlap_found=False,
            overlap_length=0,
            confidence=1.0,
        )
