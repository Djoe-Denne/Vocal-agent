"""
Shim for FastAPI app (moved to infra_web).
"""

from tts_server.infra_web.api import app

__all__ = ["app"]
