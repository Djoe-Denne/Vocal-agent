"""
Daemon adapter for PTT (hotkey + API transcription).
"""

from __future__ import annotations

import time
from pathlib import Path

from ptt.application.config import load_config
from ptt.infra_cli.hotkeys import HotkeyManager
from ptt.infra_cli.recorder import StreamingRecorder
from ptt.infra_web.api_client import TranscriptionApiClient
from ptt.infra_web.openclaw import OpenClawClient
from ptt.utils.logging import setup_logging, get_logger


class PTTDaemon:
    def __init__(self) -> None:
        self.config = load_config()
        log_dir = self.config.openclaw_shared_dir / "logs"
        setup_logging(log_dir)
        self._log = get_logger("daemon")

        self.recorder = StreamingRecorder(self.config)
        self.hotkeys = HotkeyManager()
        self.api = TranscriptionApiClient(self.config)
        self.openclaw = OpenClawClient(self.config, debug_mode=False)

    def run(self) -> None:
        self._print_banner()
        self.hotkeys.register(self.config.hotkey_toggle, self._on_toggle)
        self.hotkeys.register(self.config.hotkey_attach_image, self._on_attach_image)
        self.hotkeys.set_shutdown_callback(self._on_shutdown)
        self.hotkeys.start()

    def _print_banner(self) -> None:
        self._log.info("=" * 50)
        self._log.info("  Push-to-Talk Daemon (Hotkey + API)")
        self._log.info("=" * 50)
        self._log.info(f"  API: {self.config.daemon.api_url}")
        self._log.info(f"  {self.config.hotkey_toggle} = toggle recording")
        self._log.info(f"  {self.config.hotkey_attach_image} = attach clipboard image")
        self._log.info("  Escape or Ctrl+C = exit")
        self._log.info("=" * 50)

    def _on_toggle(self) -> None:
        if self.recorder.is_recording:
            self._stop_recording()
        else:
            self._start_recording()

    def _start_recording(self) -> None:
        self.recorder.start()

    def _stop_recording(self) -> None:
        audio_data = self.recorder.stop()
        if audio_data is None:
            self._log.warning("No audio recorded.")
            return

        ts = int(time.time())
        audio_path = Path(self.config.tmp_dir) / f"ptt_daemon_{ts}.wav"
        if not self.recorder.save_audio(audio_path):
            self._log.error("Failed to save audio for API transcription.")
            return

        text = self.api.transcribe(audio_path)
        if text:
            self._log.info("")
            self._log.info("=" * 40)
            self._log.info("FINAL TRANSCRIPTION:")
            self._log.info("=" * 40)
            print(text)
            self._log.info("=" * 40)
            self._log.info("")

            if self.openclaw.is_enabled:
                self.openclaw.send(
                    text, image_path=self.recorder.clipboard_image_path
                )
        else:
            self._log.warning("Daemon API returned no transcription.")

        if self.config.daemon.delete_audio_after_send:
            try:
                audio_path.unlink(missing_ok=True)
            except Exception as exc:
                self._log.debug(f"Failed to delete temp audio: {exc}")

    def _on_attach_image(self) -> None:
        self.recorder.attach_clipboard_image()

    def _on_shutdown(self) -> None:
        self._log.info("Cleaning up...")
        if self.recorder.is_recording:
            self.recorder.stop()
        self._log.info("Goodbye!")


def run_daemon() -> None:
    PTTDaemon().run()
