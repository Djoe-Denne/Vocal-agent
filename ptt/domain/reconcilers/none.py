"""
No-op reconciler (domain).
"""

from ptt.domain.reconciler import BaseReconciler
from ptt.domain.models import ReconciliationResult


class NoOpReconciler(BaseReconciler):
    def reconcile(self, previous_text: str, current_text: str) -> ReconciliationResult:
        return ReconciliationResult(
            new_text=current_text,
            overlap_found=False,
            overlap_length=0,
            confidence=1.0,
        )
