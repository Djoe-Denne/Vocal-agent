"""
PTT Word Overlap Reconciler (domain).
"""

from ptt.domain.reconciler import BaseReconciler
from ptt.domain.models import ReconciliationResult


class WordOverlapReconciler(BaseReconciler):
    def __init__(self, min_overlap_words: int = 3, max_context_words: int = 15):
        super().__init__()
        self.min_overlap_words = min_overlap_words
        self.max_context_words = max_context_words

    def reconcile(self, previous_text: str, current_text: str) -> ReconciliationResult:
        if not previous_text or not current_text:
            return ReconciliationResult(
                new_text=current_text,
                overlap_found=False,
                overlap_length=0,
                confidence=1.0,
            )

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
        best_overlap_len = 0
        max_possible_overlap = min(len(context_words), len(curr_words))

        for overlap_len in range(max_possible_overlap, self.min_overlap_words - 1, -1):
            prev_end = context_words[-overlap_len:]
            curr_start = curr_words[:overlap_len]
            if prev_end == curr_start:
                best_overlap_len = overlap_len
                break

        if best_overlap_len >= self.min_overlap_words:
            new_words = curr_words_original[best_overlap_len:]
            new_text = " ".join(new_words)
            return ReconciliationResult(
                new_text=new_text,
                overlap_found=True,
                overlap_length=best_overlap_len,
                confidence=1.0,
            )

        return ReconciliationResult(
            new_text=current_text,
            overlap_found=False,
            overlap_length=0,
            confidence=1.0,
        )

    def _find_partial_overlap(self, prev_words: list[str], curr_words: list[str]) -> int:
        for start_idx in range(len(prev_words)):
            remaining = prev_words[start_idx:]
            if len(remaining) > len(curr_words):
                continue

            matches = True
            for i, word in enumerate(remaining):
                if i >= len(curr_words):
                    break
                if word != curr_words[i]:
                    if i == 0 and curr_words[i].startswith(word[-3:]):
                        continue
                    matches = False
                    break

            if matches:
                return len(remaining)

        return 0
