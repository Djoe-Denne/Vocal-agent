"""
Whisper ASR transcriber adapter.

Implements BaseTranscriber using OpenAI's Whisper model.
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


class WhisperTranscriber(BaseTranscriber):
    """Transcriber using OpenAI Whisper model."""

    def __init__(self, config: Config) -> None:
        self.config = config
        self._model: Any = None
        self._model_lock = threading.Lock()
        self._log = get_logger("transcriber.whisper")

        self._device = "cuda" if torch.cuda.is_available() else "cpu"
        if self._device == "cpu":
            self._log.warning("No GPU detected - using CPU (will be slow!)")

    @property
    def is_loaded(self) -> bool:
        return self._model is not None

    def load_model(self) -> bool:
        import whisper

        with self._model_lock:
            if self._model is not None:
                self._log.info("Whisper model already loaded")
                return False

            model_name = self.config.asr.whisper.model
            self._log.info(f"Loading Whisper model '{model_name}'...")
            start = time.time()

            try:
                self._model = whisper.load_model(model_name, device=self._device)
                elapsed = time.time() - start
                self._log.info(f"Whisper model loaded on {self._device.upper()} in {elapsed:.1f}s")

                if torch.cuda.is_available():
                    mem_allocated = torch.cuda.memory_allocated() / (1024**3)
                    mem_reserved = torch.cuda.memory_reserved() / (1024**3)
                    self._log.info(
                        f"GPU memory: {mem_allocated:.2f} GB allocated, {mem_reserved:.2f} GB reserved"
                    )

                return True

            except Exception as e:
                self._log.error(f"Failed to load Whisper model: {e}")
                self._log.debug(traceback.format_exc())
                self._model = None
                return False

    def unload_model(self) -> bool:
        with self._model_lock:
            if self._model is None:
                self._log.info("Whisper model not loaded")
                return False

            self._log.info("Unloading Whisper model...")

            try:
                del self._model
                self._model = None

                if torch.cuda.is_available():
                    torch.cuda.empty_cache()
                    torch.cuda.synchronize()

                self._log.info("Whisper model unloaded, GPU memory freed")
                return True

            except Exception as e:
                self._log.error(f"Failed to unload Whisper model: {e}")
                self._log.debug(traceback.format_exc())
                return False

    def transcribe_file(self, audio_path: Path) -> Optional[TranscriptionResult]:
        if self._model is None:
            self._log.error("Whisper model not loaded!")
            return None

        audio_path = Path(audio_path)
        if not audio_path.exists():
            self._log.error(f"Audio file not found: {audio_path}")
            return None

        self._log.info(f"Transcribing: {audio_path.name}")
        start_time = time.time()

        try:
            fp16 = not self.config.asr.whisper.force_fp32 and torch.cuda.is_available()

            transcribe_kwargs: dict = {
                "language": self.config.asr.whisper.language,
                "fp16": fp16,
                "verbose": False,
            }

            if self.config.asr.whisper.initial_prompt:
                transcribe_kwargs["initial_prompt"] = self.config.asr.whisper.initial_prompt

            if self.config.asr.whisper.suppress_fillers:
                transcribe_kwargs["no_speech_threshold"] = 0.6
                transcribe_kwargs["condition_on_previous_text"] = False

            result = self._model.transcribe(str(audio_path), **transcribe_kwargs)

            elapsed = time.time() - start_time
            text = result["text"].strip()

            audio_duration = 0.0
            if result.get("segments"):
                audio_duration = result["segments"][-1].get("end", 0.0)

            self._log.info(f"Transcription complete in {elapsed:.1f}s ({len(text)} chars)")

            return TranscriptionResult(
                text=text,
                segments=result.get("segments", []),
                duration=elapsed,
                audio_duration=audio_duration,
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
            self._log.error("Whisper model not loaded!")
            return None

        audio_duration = len(audio_data) / sample_rate
        self._log.debug(f"Transcribing {audio_duration:.1f}s of audio")
        start_time = time.time()

        try:
            audio_float = prepare_audio(audio_data, sample_rate)

            fp16 = not self.config.asr.whisper.force_fp32 and torch.cuda.is_available()

            transcribe_kwargs: dict = {
                "language": self.config.asr.whisper.language,
                "fp16": fp16,
                "verbose": False,
            }

            if self.config.asr.whisper.initial_prompt:
                transcribe_kwargs["initial_prompt"] = self.config.asr.whisper.initial_prompt

            if self.config.asr.whisper.suppress_fillers:
                transcribe_kwargs["no_speech_threshold"] = 0.6
                transcribe_kwargs["condition_on_previous_text"] = False

            result = self._model.transcribe(audio_float, **transcribe_kwargs)

            elapsed = time.time() - start_time
            text = result["text"].strip()

            if on_segment and result.get("segments"):
                for seg in result["segments"]:
                    on_segment(seg)

            self._log.debug(
                f"Transcription: {text[:100]}..." if len(text) > 100 else f"Transcription: {text}"
            )

            return TranscriptionResult(
                text=text,
                segments=result.get("segments", []),
                duration=elapsed,
                audio_duration=audio_duration,
            )

        except Exception as e:
            self._log.error(f"Transcription failed: {e}")
            self._log.debug(traceback.format_exc())
            return None
