"""
CLI runner for PTT HTTP API.
"""

from __future__ import annotations

import argparse

import uvicorn

from ptt.application.config import load_config


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="PTT Transcription API")
    parser.add_argument("--host", type=str, default=None, help="Bind host")
    parser.add_argument("--port", type=int, default=None, help="Bind port")
    parser.add_argument("--reload", action="store_true", help="Enable auto-reload")
    return parser.parse_args()


def run_api_server() -> None:
    args = parse_args()
    config = load_config()
    host = args.host or config.api.host
    port = args.port or config.api.port

    uvicorn.run(
        "ptt.infra_web.api:app",
        host=host,
        port=port,
        log_level=config.api.log_level,
        reload=args.reload,
    )
