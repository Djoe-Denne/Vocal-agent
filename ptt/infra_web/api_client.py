"""
API client for daemon transcription.
"""

from __future__ import annotations

from pathlib import Path
from typing import Optional

import requests

from ptt.application.config import Config
from ptt.utils.logging import get_logger


class TranscriptionApiClient:
    def __init__(self, config: Config) -> None:
        self._config = config
        self._log = get_logger("daemon_api")

    def transcribe(self, audio_path: Path) -> Optional[str]:
        daemon = self._config.daemon
        if not daemon.api_url:
            self._log.error("Daemon API URL is not configured.")
            return None

        data = dict(daemon.api_extra_fields)
        if daemon.api_model:
            data["model"] = daemon.api_model
        if daemon.api_language:
            data["language"] = daemon.api_language

        headers = dict(daemon.api_headers)
        files = {
            daemon.api_file_field: (
                audio_path.name,
                audio_path.open("rb"),
                "audio/wav",
            )
        }

        try:
            response = requests.post(
                daemon.api_url,
                data=data,
                files=files,
                headers=headers,
                timeout=daemon.api_timeout,
            )
        except Exception as exc:
            self._log.error(f"Daemon API request failed: {exc}")
            return None
        finally:
            files[daemon.api_file_field][1].close()

        if response.status_code >= 400:
            self._log.error(
                f"Daemon API error {response.status_code}: {response.text[:200]}"
            )
            return None

        content_type = response.headers.get("content-type", "")
        if "application/json" in content_type:
            try:
                payload = response.json()
            except Exception as exc:
                self._log.error(f"Failed to parse daemon API JSON: {exc}")
                return None
            text = payload.get(daemon.api_response_key)
            if text is None:
                self._log.error(
                    f"Daemon API response missing key '{daemon.api_response_key}'."
                )
                return None
            return str(text)

        return response.text.strip()
