"""
PTT application orchestrator.
"""

from __future__ import annotations

import threading
import time

import torch

from ptt.application.config import load_config
from ptt.application.factories import create_reconciler, create_transcriber
from ptt.domain.ports import BaseReconciler, BaseTranscriber, clean_transcription
from ptt.infra_pynput.hotkeys import HotkeyManager
from ptt.infra_podman.openclaw import OpenClawClient
from ptt.infra_reconcilers.llm import LLMReconciler
from ptt.infra_sounddevice.recorder import StreamingRecorder, AudioChunk
from ptt.utils.logging import setup_logging, get_logger


class PTTApplication:
    """
    Main Push-to-Talk application orchestrator.
    """

    def __init__(self, debug_mode: bool = False):
        self.debug_mode = debug_mode
        self.config = load_config()

        log_dir = self.config.openclaw_shared_dir / "logs"
        setup_logging(log_dir)
        self._log = get_logger("app")

        self.recorder = StreamingRecorder(self.config)
        self.transcriber: BaseTranscriber = create_transcriber(self.config)
        self.reconciler: BaseReconciler = create_reconciler(self.config)
        self.openclaw = OpenClawClient(self.config, debug_mode=debug_mode)
        self.hotkeys = HotkeyManager()

        self._transcription_thread: threading.Thread | None = None
        self._stop_transcription = threading.Event()
        self._transcription_done = threading.Event()
        self._streaming_enabled = (
            self.config.reconciler.algorithm != "none"
            and self.config.streaming.chunk_duration > 0
        )

    def run(self) -> None:
        self._print_banner()
        self._check_gpu()

        self._log.info("")
        self._log.info(f"  ASR Backend: {self.config.asr.backend}")
        if self.config.asr.backend == "huggingface":
            self._log.info(f"  Model: {self.config.asr.huggingface.model}")
        else:
            self._log.info(f"  Model: {self.config.asr.whisper.model}")
        self.transcriber.load_model()

        if isinstance(self.reconciler, LLMReconciler):
            self._log.info("")
            self._log.info("Loading LLM reconciler model...")
            self.reconciler.load_model()

        self._log.info("")

        self.hotkeys.register(self.config.hotkey_toggle, self._on_toggle)
        self.hotkeys.register(self.config.hotkey_attach_image, self._on_attach_image)
        self.hotkeys.register(self.config.hotkey_unload_model, self._on_model_toggle)
        self.hotkeys.set_shutdown_callback(self._on_shutdown)

        self.hotkeys.start()

    def _print_banner(self) -> None:
        self._log.info("=" * 50)
        self._log.info("  Push-to-Talk Speech-to-Text v2.0 (Streaming)")
        if self.debug_mode:
            self._log.info("  *** DEBUG MODE ENABLED ***")
        self._log.info("=" * 50)
        self._log.info(f"  Log dir: {self.config.openclaw_shared_dir / 'logs'}")
        self._log.info(f"  {self.config.hotkey_toggle} = toggle recording")
        self._log.info(f"  {self.config.hotkey_attach_image} = attach clipboard image")
        self._log.info(f"  {self.config.hotkey_unload_model} = toggle model load/unload")
        self._log.info("  Escape or Ctrl+C = exit")
        self._log.info("=" * 50)
        self._log.info(f"  Reconciler: {self.config.reconciler.algorithm}")
        if self._streaming_enabled:
            self._log.info(
                f"  Chunk: {self.config.streaming.chunk_duration}s, Overlap: {self.config.streaming.overlap_duration}s"
            )
        else:
            self._log.info("  Chunk: disabled (no streaming)")
        self._log.info("=" * 50)

    def _check_gpu(self) -> None:
        if torch.cuda.is_available():
            gpu_name = torch.cuda.get_device_name(0)
            gpu_mem = torch.cuda.get_device_properties(0).total_memory / (1024**3)
            self._log.info(f"  GPU: {gpu_name} ({gpu_mem:.1f} GB)")
        else:
            self._log.warning("=" * 50)
            self._log.warning("  NO GPU DETECTED - USING CPU (SLOW!)")
            self._log.warning("  Install PyTorch with CUDA support:")
            self._log.warning(
                "  pip install torch torchvision --index-url https://download.pytorch.org/whl/cu130"
            )
            self._log.warning("=" * 50)

    def _on_toggle(self) -> None:
        self._log.debug("Toggle hotkey triggered")
        if self.recorder.is_recording:
            self._stop_recording()
        else:
            self._start_recording()

    def _start_recording(self) -> None:
        self.reconciler.reset()
        self._stop_transcription.clear()
        self._transcription_done.clear()

        if self._streaming_enabled:
            if self.recorder.start(on_chunk=self._on_audio_chunk):
                self._transcription_thread = threading.Thread(
                    target=self._transcription_loop, daemon=True
                )
                self._transcription_thread.start()
        else:
            self.recorder.start()

    def _stop_recording(self) -> None:
        audio_data = self.recorder.stop()

        if self._streaming_enabled:
            self._stop_transcription.set()
            if self._transcription_thread:
                self._transcription_thread.join()
            full_text = self.reconciler.get_full_text()
        else:
            full_text = ""
            if audio_data is not None:
                result = self.transcriber.transcribe_array(audio_data, self.config.rate)
                if result and result.text:
                    full_text = clean_transcription(result.text)

        if full_text:
            self._log.info("")
            self._log.info("=" * 40)
            self._log.info("FINAL TRANSCRIPTION:")
            self._log.info("=" * 40)
            print(full_text)
            self._log.info("=" * 40)
            self._log.info("")
            self.openclaw.send(
                full_text, image_path=self.recorder.clipboard_image_path
            )
        else:
            self._log.warning("No transcription produced")

    def _on_audio_chunk(self, chunk: AudioChunk) -> None:
        self._log.debug(
            f"Audio chunk received: {chunk.chunk_index}, {len(chunk.data) / chunk.sample_rate:.1f}s"
        )

    def _transcription_loop(self) -> None:
        final_received = False
        try:
            while True:
                chunk = self.recorder.get_chunk(timeout=0.5)

                if chunk is None:
                    if self._stop_transcription.is_set() and final_received:
                        break
                    continue

                result = self.transcriber.transcribe_array(
                    chunk.data, chunk.sample_rate
                )

                if result and result.text:
                    reconciliation = self.reconciler.add_segment(result.text)
                    if reconciliation.new_text:
                        print(reconciliation.new_text, end=" ", flush=True)
                        if reconciliation.overlap_found:
                            self._log.debug(
                                f"Chunk {chunk.chunk_index}: overlap={reconciliation.overlap_length} words, "
                                f"new='{reconciliation.new_text[:50]}...'"
                            )

                if chunk.is_final:
                    final_received = True
        finally:
            print()
            self._transcription_done.set()

    def _on_attach_image(self) -> None:
        self._log.debug("Attach image hotkey triggered")
        self.recorder.attach_clipboard_image()

    def _on_model_toggle(self) -> None:
        self._log.debug("Model toggle hotkey triggered")
        if self.transcriber.is_loaded:
            self.transcriber.unload_model()
        else:
            self.transcriber.load_model()

    def _on_shutdown(self) -> None:
        self._log.info("Cleaning up...")
        if self.recorder.is_recording:
            self.recorder.stop()
        self.transcriber.unload_model()
        if hasattr(self.reconciler, "unload_model"):
            self.reconciler.unload_model()
        self._log.info("Goodbye!")
