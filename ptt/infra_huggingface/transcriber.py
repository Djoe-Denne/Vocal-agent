"""
HuggingFace ASR transcriber adapter.

Implements BaseTranscriber using HuggingFace models (e.g. Qwen3-ASR)
via the qwen-asr package.
"""

from __future__ import annotations

import threading
import time
import traceback
from pathlib import Path
from typing import Any, Callable, Optional

import numpy as np
import torch

from ptt.application.config import Config
from ptt.domain.models import TranscriptionResult
from ptt.domain.ports import BaseTranscriber
from ptt.utils.audio import prepare_audio
from ptt.utils.logging import get_logger


class HuggingFaceTranscriber(BaseTranscriber):
    """Transcriber using HuggingFace models via the qwen-asr package."""

    def __init__(self, config: Config) -> None:
        self.config = config
        self._model: Any = None
        self._model_lock = threading.Lock()
        self._log = get_logger("transcriber.huggingface")

        self._device = "cuda" if torch.cuda.is_available() else "cpu"
        if self._device == "cpu":
            self._log.warning("No GPU detected - using CPU (will be slow!)")

    @property
    def is_loaded(self) -> bool:
        return self._model is not None

    # ----- helpers -----------------------------------------------------------

    @staticmethod
    def _extract_text(result: Any) -> str:
        """Extract plain text from qwen-asr result objects."""
        if result is None:
            return ""
        if isinstance(result, str):
            return result
        text = getattr(result, "text", None)
        if isinstance(text, str):
            return text
        if isinstance(result, dict) and "text" in result:
            return str(result["text"])
        return str(result)

    def _resolve_model_device(self) -> str:
        """Best-effort device detection for the underlying model."""
        if self._model is None:
            return "unknown"
        for attr in ("model", "asr_model", "net", "nn_model"):
            candidate = getattr(self._model, attr, None)
            if candidate is not None:
                try:
                    param = next(candidate.parameters())
                    return str(param.device)
                except Exception:
                    pass
        try:
            param = next(self._model.parameters())
            return str(param.device)
        except Exception:
            return "unknown"

    # ----- port implementation -----------------------------------------------

    def load_model(self) -> bool:
        from qwen_asr import Qwen3ASRModel

        with self._model_lock:
            if self._model is not None:
                self._log.info("Qwen3-ASR model already loaded")
                return False

            hf_config = self.config.asr.huggingface
            model_id = hf_config.model
            self._log.info(f"Loading Qwen3-ASR model '{model_id}'...")
            start = time.time()

            try:
                self._log.info(
                    "CUDA available=%s, torch=%s, cuda_version=%s, device=%s",
                    torch.cuda.is_available(),
                    torch.__version__,
                    torch.version.cuda,
                    self._device,
                )
                if torch.version.cuda is None:
                    self._log.warning(
                        "PyTorch CUDA build not detected (torch.version.cuda is None). "
                        "Install a CUDA-enabled torch build to use the GPU."
                    )
                if torch.cuda.is_available():
                    gpu_name = torch.cuda.get_device_name(0)
                    gpu_mem = torch.cuda.get_device_properties(0).total_memory / (1024**3)
                    self._log.info("GPU: %s (%.1f GB total)", gpu_name, gpu_mem)

                dtype_map = {
                    "float16": torch.float16,
                    "bfloat16": torch.bfloat16,
                    "float32": torch.float32,
                }
                torch_dtype = dtype_map.get(hf_config.torch_dtype, torch.float16)

                if not torch.cuda.is_available():
                    torch_dtype = torch.float32

                device_map = (
                    "auto" if (torch.cuda.is_available() and hf_config.device_map_auto) else None
                )

                model_kwargs = {
                    "torch_dtype": torch_dtype,
                    "device_map": device_map,
                }

                self._model = Qwen3ASRModel.from_pretrained(model_id, **model_kwargs)

                if device_map is None and torch.cuda.is_available():
                    try:
                        self._model.model = self._model.model.to(self._device)
                    except Exception as e:
                        self._log.warning("Failed to move model to %s: %s", self._device, e)

                elapsed = time.time() - start
                self._log.info(f"Qwen3-ASR model loaded on {self._device.upper()} in {elapsed:.1f}s")
                self._log.info("Model device: %s", self._resolve_model_device())

                if torch.cuda.is_available():
                    mem_allocated = torch.cuda.memory_allocated() / (1024**3)
                    mem_reserved = torch.cuda.memory_reserved() / (1024**3)
                    self._log.info(
                        f"GPU memory: {mem_allocated:.2f} GB allocated, {mem_reserved:.2f} GB reserved"
                    )

                return True

            except Exception as e:
                self._log.error(f"Failed to load Qwen3-ASR model: {e}")
                self._log.debug(traceback.format_exc())
                self._model = None
                return False

    def unload_model(self) -> bool:
        with self._model_lock:
            if self._model is None:
                self._log.info("Qwen3-ASR model not loaded")
                return False

            self._log.info("Unloading Qwen3-ASR model...")

            try:
                del self._model
                self._model = None

                if torch.cuda.is_available():
                    torch.cuda.empty_cache()
                    torch.cuda.synchronize()

                self._log.info("Qwen3-ASR model unloaded, GPU memory freed")
                return True

            except Exception as e:
                self._log.error(f"Failed to unload Qwen3-ASR model: {e}")
                self._log.debug(traceback.format_exc())
                return False

    def transcribe_file(self, audio_path: Path) -> Optional[TranscriptionResult]:
        if self._model is None:
            self._log.error("Qwen3-ASR model not loaded!")
            return None

        audio_path = Path(audio_path)
        if not audio_path.exists():
            self._log.error(f"Audio file not found: {audio_path}")
            return None

        self._log.info(f"Transcribing: {audio_path.name}")
        start_time = time.time()

        try:
            results = self._model.transcribe([str(audio_path)])

            text = ""
            if results:
                text = self._extract_text(results[0]).strip()

            elapsed = time.time() - start_time

            import librosa

            audio_duration = librosa.get_duration(path=str(audio_path))

            segments = [{"id": 0, "start": 0.0, "end": audio_duration, "text": text}]

            self._log.debug(
                f"Transcription: {text[:100]}..." if len(text) > 100 else f"Transcription: {text}"
            )

            return TranscriptionResult(
                text=text, segments=segments, duration=elapsed, audio_duration=audio_duration
            )

        except Exception as e:
            self._log.error(f"Transcription failed: {e}")
            self._log.debug(traceback.format_exc())
            return None

    def transcribe_array(
        self,
        audio_data: np.ndarray,
        sample_rate: int,
        on_segment: Optional[Callable[[dict], None]] = None,
    ) -> Optional[TranscriptionResult]:
        if self._model is None:
            self._log.error("Qwen3-ASR model not loaded!")
            return None

        audio_duration = len(audio_data) / sample_rate
        self._log.debug(f"Transcribing {audio_duration:.1f}s of audio")
        start_time = time.time()

        try:
            audio_float = prepare_audio(audio_data, sample_rate)

            results = self._model.transcribe([(audio_float, 16000)])

            text = ""
            if results:
                text = self._extract_text(results[0]).strip()

            elapsed = time.time() - start_time

            segments = [{"id": 0, "start": 0.0, "end": audio_duration, "text": text}]

            if on_segment:
                for seg in segments:
                    on_segment(seg)

            self._log.debug(
                f"Transcription: {text[:100]}..." if len(text) > 100 else f"Transcription: {text}"
            )

            return TranscriptionResult(
                text=text, segments=segments, duration=elapsed, audio_duration=audio_duration
            )

        except Exception as e:
            self._log.error(f"Transcription failed: {e}")
            self._log.debug(traceback.format_exc())
            return None
