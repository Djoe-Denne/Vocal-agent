"""
Sounddevice audio recording adapter.

Implements audio recording with real-time chunking for streaming transcription.
Records audio in configurable chunks with overlap for accurate transcription.
"""

import queue
import threading
import time
import traceback
from dataclasses import dataclass
from pathlib import Path
from typing import Optional, Callable

import numpy as np
import sounddevice as sd
from scipy.io import wavfile

from ptt.application.config import Config
from ptt.utils.logging import get_logger
from ptt.utils.audio import play_beep


@dataclass
class AudioChunk:
    """Represents a chunk of audio data for transcription."""
    data: np.ndarray
    sample_rate: int
    chunk_index: int
    timestamp: float
    is_final: bool = False


class StreamingRecorder:
    """
    Audio recorder with real-time chunking for streaming transcription.

    Records audio continuously and emits chunks of configurable duration
    with configurable overlap.
    """

    def __init__(self, config: Config):
        self.config = config
        self._log = get_logger("recorder")

        # Recording state
        self._recording = False
        self._stream: Optional[sd.InputStream] = None
        self._lock = threading.Lock()

        # Audio buffer
        self._audio_buffer: list[np.ndarray] = []
        self._buffer_lock = threading.Lock()

        # Chunk settings
        self._chunk_duration = config.streaming.chunk_duration
        self._overlap_duration = config.streaming.overlap_duration
        self._chunking_enabled = self._chunk_duration > 0
        self._chunk_samples = int(config.rate * self._chunk_duration) if self._chunking_enabled else 0
        self._overlap_samples = int(config.rate * self._overlap_duration) if self._chunking_enabled else 0

        # Chunk queue for consumers
        self._chunk_queue: queue.Queue[AudioChunk] = queue.Queue()
        self._chunk_callback: Optional[Callable[[AudioChunk], None]] = None
        self._chunk_index = 0

        # Beep control
        self._beep_stop = threading.Event()
        self._beep_thread: Optional[threading.Thread] = None

        # Chunking thread
        self._chunker_thread: Optional[threading.Thread] = None
        self._chunker_stop = threading.Event()

        # Previous chunk's overlap data
        self._previous_overlap: Optional[np.ndarray] = None

        # Track position
        self._last_chunk_end: int = 0

        # Clipboard image
        self.clipboard_image_path: Optional[Path] = None

    @property
    def is_recording(self) -> bool:
        return self._recording

    def start(self, on_chunk: Optional[Callable[[AudioChunk], None]] = None) -> bool:
        with self._lock:
            if self._recording:
                self._log.debug("Already recording, ignoring start request")
                return False

            try:
                self._audio_buffer = []
                self._chunk_index = 0
                self._previous_overlap = None
                self._last_chunk_end = 0
                self._chunk_callback = on_chunk
                self.clipboard_image_path = None

                while not self._chunk_queue.empty():
                    try:
                        self._chunk_queue.get_nowait()
                    except queue.Empty:
                        break

                self._log.debug(
                    f"Starting audio stream: rate={self.config.rate}, channels={self.config.channels}"
                )
                self._stream = sd.InputStream(
                    samplerate=self.config.rate,
                    channels=self.config.channels,
                    dtype=np.int16,
                    callback=self._audio_callback,
                )
                self._stream.start()
                self._recording = True

                if self._chunking_enabled:
                    self._chunker_stop.clear()
                    self._chunker_thread = threading.Thread(target=self._chunker_loop, daemon=True)
                    self._chunker_thread.start()

                if self.config.beep_start_stop:
                    threading.Thread(target=self._play_beep, daemon=True).start()

                self._beep_stop.clear()
                self._beep_thread = threading.Thread(target=self._beep_loop, daemon=True)
                self._beep_thread.start()

                if self._chunking_enabled:
                    self._log.info(
                        f"Recording started (chunk: {self._chunk_duration}s, overlap: {self._overlap_duration}s)"
                    )
                else:
                    self._log.info("Recording started (streaming disabled)")
                return True

            except Exception as e:
                self._log.error(f"Failed to start recording: {e}")
                self._log.debug(traceback.format_exc())
                self._recording = False
                return False

    def stop(self) -> Optional[np.ndarray]:
        with self._lock:
            if not self._recording:
                self._log.debug("Not recording, ignoring stop request")
                return None

            try:
                self._beep_stop.set()

                if self._chunking_enabled:
                    self._chunker_stop.set()
                    if self._chunker_thread:
                        self._chunker_thread.join(timeout=1.0)

                if self._stream:
                    self._stream.stop()
                    self._stream.close()
                    self._stream = None

                self._recording = False

                if self.config.beep_start_stop:
                    threading.Thread(target=self._play_beep, daemon=True).start()

                if self._chunking_enabled:
                    self._emit_final_chunk()

                with self._buffer_lock:
                    if not self._audio_buffer:
                        self._log.warning("No audio data recorded")
                        return None

                    audio_data = np.concatenate(self._audio_buffer, axis=0)
                    duration = len(audio_data) / self.config.rate
                    self._log.info(
                        f"Recording stopped: {duration:.1f}s total, {self._chunk_index} chunks"
                    )
                    return audio_data

            except Exception as e:
                self._log.error(f"Error stopping recording: {e}")
                self._log.debug(traceback.format_exc())
                return None

    def get_chunk(self, timeout: Optional[float] = None) -> Optional[AudioChunk]:
        try:
            return self._chunk_queue.get(timeout=timeout)
        except queue.Empty:
            return None

    def attach_clipboard_image(self) -> Optional[Path]:
        if not self._recording:
            self._log.warning("Cannot attach image - not recording")
            return None

        try:
            from PIL import ImageGrab

            img = ImageGrab.grabclipboard()
            if img is not None:
                ts = int(time.time())
                out = self.config.openclaw_shared_dir / f"{self.config.clipboard_prefix}{ts}.png"
                img.save(str(out), "PNG")

                self.clipboard_image_path = out
                self._play_beep()
                self._log.info(f"Clipboard image attached -> {out}")
                return out
            else:
                self._log.info("No image in clipboard")
                return None

        except Exception as e:
            self._log.error(f"Failed to attach clipboard image: {e}")
            self._log.debug(traceback.format_exc())
            return None

    def save_audio(self, path: Path) -> bool:
        with self._buffer_lock:
            if not self._audio_buffer:
                self._log.warning("No audio to save")
                return False

            try:
                audio_data = np.concatenate(self._audio_buffer, axis=0)
                wavfile.write(str(path), self.config.rate, audio_data)

                size = path.stat().st_size
                duration = len(audio_data) / self.config.rate
                self._log.info(f"Audio saved -> {path} ({size} bytes, {duration:.1f}s)")
                return True

            except Exception as e:
                self._log.error(f"Failed to save audio: {e}")
                self._log.debug(traceback.format_exc())
                return False

    def get_full_audio(self) -> Optional[np.ndarray]:
        with self._buffer_lock:
            if not self._audio_buffer:
                return None
            return np.concatenate(self._audio_buffer, axis=0)

    # -- internal callbacks / threads ------------------------------------------

    def _audio_callback(self, indata, frames, time_info, status):
        if status:
            self._log.warning(f"Audio stream status: {status}")
        with self._buffer_lock:
            self._audio_buffer.append(indata.copy())

    def _chunker_loop(self):
        while not self._chunker_stop.wait(timeout=0.5):
            with self._buffer_lock:
                if not self._audio_buffer:
                    continue

                total_samples = sum(len(chunk) for chunk in self._audio_buffer)
                samples_needed = self._last_chunk_end + self._chunk_samples

                if total_samples >= samples_needed:
                    all_audio = np.concatenate(self._audio_buffer, axis=0)
                    chunk_start = self._last_chunk_end
                    chunk_end = chunk_start + self._chunk_samples
                    chunk_data = all_audio[chunk_start:chunk_end]

                    if self._previous_overlap is not None:
                        chunk_data = np.concatenate([self._previous_overlap, chunk_data], axis=0)

                    overlap_start = max(0, chunk_end - self._overlap_samples)
                    self._previous_overlap = all_audio[overlap_start:chunk_end].copy()

                    self._last_chunk_end = chunk_end
                    self._emit_chunk(chunk_data, is_final=False)

    def _emit_final_chunk(self):
        if not self._chunking_enabled:
            return
        with self._buffer_lock:
            if not self._audio_buffer:
                return

            all_audio = np.concatenate(self._audio_buffer, axis=0)
            total_samples = len(all_audio)

            if total_samples <= self._last_chunk_end:
                self._log.debug("No remaining audio for final chunk")
                return

            remaining_audio = all_audio[self._last_chunk_end:]

            if self._previous_overlap is not None:
                remaining_audio = np.concatenate([self._previous_overlap, remaining_audio], axis=0)

            remaining_duration = len(remaining_audio) / self.config.rate
            self._log.debug(f"Final chunk: {remaining_duration:.1f}s of remaining audio")

            if len(remaining_audio) > 0:
                self._emit_chunk(remaining_audio, is_final=True)

    def _emit_chunk(self, data: np.ndarray, is_final: bool):
        chunk = AudioChunk(
            data=data,
            sample_rate=self.config.rate,
            chunk_index=self._chunk_index,
            timestamp=time.time(),
            is_final=is_final,
        )

        self._chunk_index += 1

        duration = len(data) / self.config.rate
        self._log.debug(f"Chunk {chunk.chunk_index}: {duration:.1f}s (final={is_final})")

        self._chunk_queue.put(chunk)

        if self._chunk_callback:
            try:
                self._chunk_callback(chunk)
            except Exception as e:
                self._log.error(f"Chunk callback error: {e}")

    def _play_beep(self):
        play_beep(self.config.beep_frequency, self.config.beep_duration)

    def _beep_loop(self):
        while not self._beep_stop.wait(self.config.beep_every):
            self._play_beep()
