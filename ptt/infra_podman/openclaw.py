"""
OpenClaw client adapter (Podman container).

Client for sending transcriptions to OpenClaw AI agent via Podman container.
"""

import subprocess
import traceback
from pathlib import Path
from typing import Optional

from ptt.application.config import Config
from ptt.utils.logging import get_logger


class OpenClawClient:
    """
    Client for communicating with OpenClaw AI agent running in a Podman container.
    """

    def __init__(self, config: Config, debug_mode: bool = False):
        self.config = config
        self.debug_mode = debug_mode
        self._log = get_logger("openclaw")

    @property
    def is_enabled(self) -> bool:
        return self.config.openclaw_send

    @property
    def container_name(self) -> str:
        return self.config.openclaw_container_name

    def send(
        self,
        message: str,
        image_path: Optional[Path] = None,
        timeout: int = 60,
    ) -> bool:
        if not self.is_enabled:
            self._log.debug("OpenClaw send disabled, skipping")
            return False

        formatted_msg = self._format_message(message, image_path)

        if self.debug_mode:
            confirmed, edited_msg = self._debug_confirm(formatted_msg)
            if not confirmed:
                self._log.info("DEBUG: User cancelled sending to OpenClaw")
                return False
            formatted_msg = edited_msg

        return self._execute_send(formatted_msg, timeout)

    # -- internal helpers ------------------------------------------------------

    def _format_message(self, message: str, image_path: Optional[Path] = None) -> str:
        msg = message
        if self.config.openclaw_single_line:
            msg = " ".join(msg.splitlines())
        if len(msg) > self.config.openclaw_max_chars:
            self._log.debug(
                f"Truncating message from {len(msg)} to {self.config.openclaw_max_chars} chars"
            )
            msg = msg[: self.config.openclaw_max_chars]
        if image_path:
            container_shared_path = "/app/shared"
            image_filename = image_path.name
            container_image_path = f"{container_shared_path}/{image_filename}"
            msg += f"\n\n[clipboard_image]: {container_image_path}"
            self._log.debug(f"Added image reference: {container_image_path}")
        return msg

    def _debug_confirm(self, message: str) -> tuple[bool, str]:
        print("\n" + "=" * 60)
        print("DEBUG MODE - Message to send to OpenClaw:")
        print("=" * 60)
        print(message)
        print("=" * 60)
        print(f"Container: {self.config.openclaw_container_name}")
        print(f"Session: {self.config.openclaw_session_id}")
        print(f"Length: {len(message)} chars")
        print("=" * 60)

        while True:
            try:
                response = input("\nSend to OpenClaw? [y/n/e(dit)]: ").strip().lower()

                if response in ("n", "no"):
                    print("[CANCELLED] Not sent to OpenClaw")
                    return (False, message)
                elif response in ("y", "yes", ""):
                    self._log.info("DEBUG: User confirmed sending to OpenClaw")
                    return (True, message)
                elif response in ("e", "edit"):
                    print("\nEnter new message (end with empty line):")
                    lines = []
                    while True:
                        line = input()
                        if line == "":
                            break
                        lines.append(line)
                    if lines:
                        message = "\n".join(lines)
                        print(f"\n[UPDATED] New message ({len(message)} chars)")
                        self._log.info(
                            f"DEBUG: User edited message, new length: {len(message)} chars"
                        )
                    continue
                else:
                    print("Please enter 'y' (yes), 'n' (no), or 'e' (edit)")
            except EOFError:
                self._log.info("DEBUG: EOF received, cancelling")
                return (False, message)

    def _execute_send(self, message: str, timeout: int) -> bool:
        cmd = [
            "podman",
            "exec",
            self.config.openclaw_container_name,
            "openclaw",
            "agent",
            "--message",
            message,
            "--session-id",
            self.config.openclaw_session_id,
        ]

        self._log.info(f"Sending to OpenClaw container '{self.config.openclaw_container_name}'...")
        self._log.debug(f"Message length: {len(message)} chars")

        try:
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
                timeout=timeout,
            )

            if result.returncode != 0:
                self._log.error(f"OpenClaw failed (code {result.returncode}): {result.stderr}")
                return False

            self._log.info("OpenClaw: sent successfully")
            if result.stdout:
                self._log.debug(f"OpenClaw response: {result.stdout[:200]}")
            return True

        except subprocess.TimeoutExpired:
            self._log.error(f"OpenClaw command timed out after {timeout} seconds")
            return False
        except FileNotFoundError:
            self._log.error("Podman not found. Is it installed and in PATH?")
            return False
        except Exception as e:
            self._log.error(f"OpenClaw error: {e}")
            self._log.debug(traceback.format_exc())
            return False

    def check_container(self) -> bool:
        try:
            result = subprocess.run(
                [
                    "podman",
                    "ps",
                    "--filter",
                    f"name={self.config.openclaw_container_name}",
                    "--format",
                    "{{.Names}}",
                ],
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
                timeout=10,
            )
            return self.config.openclaw_container_name in result.stdout
        except Exception as e:
            self._log.debug(f"Container check failed: {e}")
            return False

    def get_container_status(self) -> dict:
        status = {"name": self.config.openclaw_container_name, "running": False, "error": None}
        try:
            result = subprocess.run(
                [
                    "podman",
                    "inspect",
                    "--format",
                    "{{.State.Running}}",
                    self.config.openclaw_container_name,
                ],
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
                timeout=10,
            )
            if result.returncode == 0:
                status["running"] = result.stdout.strip().lower() == "true"
            else:
                status["error"] = result.stderr.strip()
        except FileNotFoundError:
            status["error"] = "Podman not found"
        except subprocess.TimeoutExpired:
            status["error"] = "Timeout checking container"
        except Exception as e:
            status["error"] = str(e)
        return status
