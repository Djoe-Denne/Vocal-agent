"""
LLM-based reconciler adapter.
"""

import threading
import traceback
from typing import Optional

from ptt.domain.models import ReconciliationResult
from ptt.domain.ports import BaseReconciler
from ptt.utils.logging import get_logger


class LLMReconciler(BaseReconciler):
    MERGE_PROMPT_CLEANUP = """You are a speech transcription cleaner. Given two overlapping transcription segments, merge them and clean up the result.

Previous segment: "{previous}"
Current segment: "{current}"

Tasks:
1. Find and remove duplicate words/phrases at the overlap point
2. Remove hesitations (uh, um, euh, eh, ah, hmm, etc.)
3. Remove stutters and repetitions (e.g., "le le" -> "le", "si le si" -> "si")
4. Remove filler words (like "donc euh", "ben", "genre", "you know", "like")
5. Keep the meaning intact, just clean up the speech artifacts
6. Output ONLY the new (non-duplicate) cleaned words from the current segment
7. If no overlap, output the cleaned current segment
8. Output only the text, no explanations

Cleaned new words only:"""

    MERGE_PROMPT_SIMPLE = """You are a text merger. Given two overlapping transcription segments, merge them into one coherent text by removing duplicate words/phrases at the overlap point.

Previous segment: "{previous}"
Current segment: "{current}"

Rules:
1. Find where the segments overlap (duplicate words)
2. Remove the duplicate words from the current segment
3. Output ONLY the new (non-duplicate) words from the current segment
4. If no overlap, output the entire current segment
5. Output only the text, no explanations

New words only:"""

    def __init__(
        self,
        model_name: str = "HuggingFaceTB/SmolLM2-360M-Instruct",
        device: str = "cuda",
        max_new_tokens: int = 256,
        cleanup_hesitations: bool = True,
    ):
        super().__init__()
        self.model_name = model_name
        self.device = device
        self.max_new_tokens = max_new_tokens
        self.cleanup_hesitations = cleanup_hesitations

        self._model = None
        self._tokenizer = None
        self._model_lock = threading.Lock()
        self._transformers_available: Optional[bool] = None
        self._log = get_logger("llm_reconciler")

    def _check_transformers(self) -> bool:
        if self._transformers_available is None:
            try:
                import transformers  # noqa: F401
                self._transformers_available = True
            except ImportError:
                self._transformers_available = False
        return self._transformers_available

    def load_model(self) -> bool:
        if not self._check_transformers():
            return False

        with self._model_lock:
            if self._model is not None:
                return True

            try:
                from transformers import AutoModelForCausalLM, AutoTokenizer
                import torch

                if self.device == "cuda" and not torch.cuda.is_available():
                    self.device = "cpu"

                self._log.info(f"Loading LLM model '{self.model_name}' on {self.device.upper()}...")

                self._tokenizer = AutoTokenizer.from_pretrained(
                    self.model_name, trust_remote_code=True
                )

                self._model = AutoModelForCausalLM.from_pretrained(
                    self.model_name,
                    dtype=torch.float16 if self.device == "cuda" else torch.float32,
                    device_map=self.device if self.device == "cuda" else None,
                    trust_remote_code=True,
                    low_cpu_mem_usage=True,
                )

                if self.device == "cpu":
                    self._model = self._model.to("cpu")

                if self.device == "cuda":
                    mem_allocated = torch.cuda.memory_allocated() / (1024**3)
                    mem_reserved = torch.cuda.memory_reserved() / (1024**3)
                    self._log.info(
                        f"LLM model loaded ({mem_allocated:.2f} GB allocated, {mem_reserved:.2f} GB reserved)"
                    )
                else:
                    self._log.info("LLM model loaded on CPU")

                return True
            except Exception as e:
                self._log.error(f"Failed to load LLM model: {e}")
                self._log.debug(traceback.format_exc())
                self._model = None
                self._tokenizer = None
                return False

    def unload_model(self) -> bool:
        with self._model_lock:
            if self._model is None:
                return False

            try:
                import torch

                del self._model
                del self._tokenizer
                self._model = None
                self._tokenizer = None

                if torch.cuda.is_available():
                    torch.cuda.empty_cache()

                self._log.info("LLM model unloaded, memory freed")
                return True
            except Exception as e:
                self._log.error(f"Error unloading LLM model: {e}")
                return False

    @property
    def is_loaded(self) -> bool:
        return self._model is not None

    def reconcile(self, previous_text: str, current_text: str) -> ReconciliationResult:
        if not previous_text or not current_text:
            return ReconciliationResult(
                new_text=current_text,
                overlap_found=False,
                overlap_length=0,
                confidence=1.0,
            )

        if not self._check_transformers():
            return self._simple_fallback(previous_text, current_text)

        if not self.is_loaded and not self.load_model():
            return self._simple_fallback(previous_text, current_text)

        try:
            import torch

            prev_words = previous_text.split()
            context = " ".join(prev_words[-20:])
            prompt_template = (
                self.MERGE_PROMPT_CLEANUP if self.cleanup_hesitations else self.MERGE_PROMPT_SIMPLE
            )
            prompt = prompt_template.format(previous=context, current=current_text)

            inputs = self._tokenizer(
                prompt, return_tensors="pt", truncation=True, max_length=512
            )

            if self.device == "cuda":
                inputs = {k: v.to("cuda") for k, v in inputs.items()}

            with torch.no_grad():
                outputs = self._model.generate(
                    **inputs,
                    max_new_tokens=self.max_new_tokens,
                    do_sample=False,
                    pad_token_id=self._tokenizer.eos_token_id,
                    eos_token_id=self._tokenizer.eos_token_id,
                )

            generated = self._tokenizer.decode(
                outputs[0][inputs["input_ids"].shape[1]:],
                skip_special_tokens=True,
            )
            new_text = generated.strip().split("\n")[0].strip()
            if new_text:
                return ReconciliationResult(
                    new_text=new_text,
                    overlap_found=True,
                    overlap_length=len(new_text.split()),
                    confidence=0.9,
                )

            return ReconciliationResult(
                new_text=current_text,
                overlap_found=False,
                overlap_length=0,
                confidence=0.0,
            )
        except Exception as e:
            self._log.error(f"LLM reconciliation failed: {e}")
            self._log.debug(traceback.format_exc())
            return self._simple_fallback(previous_text, current_text)

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

        for overlap_len in range(min(len(prev_words), len(curr_words)), 2, -1):
            prev_end = prev_words[-overlap_len:]
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
