"""
CLI entrypoint for PTT.
"""

from __future__ import annotations

import argparse
import sys


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Push-to-Talk Speech-to-Text for Windows",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
    python -m ptt --daemon              Hotkey daemon (records + API)
    python -m ptt --file .\\sample.wav  Transcribe audio file
    python -m ptt --api                 Start HTTP API server
        """,
    )
    mode_group = parser.add_mutually_exclusive_group(required=True)
    mode_group.add_argument(
        "--daemon",
        action="store_true",
        help="Run daemon mode (hotkey + API after recording)",
    )
    mode_group.add_argument(
        "--file",
        type=str,
        help="Transcribe a local audio file and print text to stdout",
    )
    mode_group.add_argument(
        "--api",
        action="store_true",
        help="Run HTTP API server (byte stream transcription)",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    try:
        if args.daemon:
            from ptt.infra_daemon.daemon import run_daemon

            run_daemon()
            return
        if args.api:
            from ptt.infra_web.cli import run_api_server

            run_api_server()
            return
        if args.file:
            from pathlib import Path

            from ptt.application.config import load_config
            from ptt.application.factories import create_transcriber

            config = load_config()
            transcriber = create_transcriber(config)
            transcriber.load_model()
            result = transcriber.transcribe_file(Path(args.file))
            if result and result.text:
                print(result.text)
            else:
                print("")
            return
    except KeyboardInterrupt:
        print("\nShutting down...")
    except Exception as exc:
        print(f"Fatal error: {exc}")
        import traceback

        traceback.print_exc()
        sys.exit(1)
