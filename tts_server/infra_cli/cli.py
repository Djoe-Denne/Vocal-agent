"""
CLI entrypoint for TTS server.
"""

from __future__ import annotations

import argparse
from pathlib import Path

import uvicorn

from tts_server.application.config import load_config


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Qwen3 TTS Server")
    parser.add_argument("--config", type=Path, default=None, help="Path to tts.toml")
    parser.add_argument("--host", type=str, default=None, help="Bind host")
    parser.add_argument("--port", type=int, default=None, help="Bind port")
    parser.add_argument("--reload", action="store_true", help="Enable auto-reload")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    config = load_config(args.config)

    host = args.host or config.server.host
    port = args.port or config.server.port

    uvicorn.run(
        "tts_server.server:app",
        host=host,
        port=port,
        log_level=config.server.log_level,
        reload=args.reload,
    )
