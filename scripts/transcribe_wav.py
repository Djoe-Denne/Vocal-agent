#!/usr/bin/env python3
"""Simple helper to call orchestration-service from a WAV file.

Modes:
- `redub-wav` (default): sends audio to `/api/asr/redub.wav` and writes WAV output.
- `transcribe`: sends audio to `/api/asr/transcribe` and prints JSON response.
"""

from __future__ import annotations

import argparse
import struct
import sys
import uuid
import wave
from pathlib import Path

import requests


def read_wav_samples(wav_path: Path) -> tuple[list[float], int]:
    with wave.open(str(wav_path), "rb") as wav_file:
        channels = wav_file.getnchannels()
        sample_width = wav_file.getsampwidth()
        sample_rate_hz = wav_file.getframerate()
        frame_count = wav_file.getnframes()
        raw = wav_file.readframes(frame_count)

    if sample_width != 2:
        raise ValueError(
            f"Unsupported sample width: {sample_width * 8} bits. "
            "Only 16-bit PCM WAV is supported."
        )
    if channels < 1:
        raise ValueError("Invalid WAV file: no channels.")

    # WAV PCM is little-endian signed 16-bit.
    all_samples = [s[0] for s in struct.iter_unpack("<h", raw)]

    # For multi-channel audio, keep the first channel.
    mono_samples = all_samples[::channels]

    # Normalize int16 to [-1.0, 1.0).
    normalized = [sample / 32768.0 for sample in mono_samples]
    return normalized, sample_rate_hz


def resample_linear(samples: list[float], src_rate_hz: int, dst_rate_hz: int) -> list[float]:
    if src_rate_hz == dst_rate_hz:
        return samples
    if src_rate_hz <= 0 or dst_rate_hz <= 0:
        raise ValueError("Sample rates must be positive.")
    if not samples:
        return samples

    out_len = int(round(len(samples) * dst_rate_hz / src_rate_hz))
    if out_len <= 1:
        return [samples[0]]

    resampled: list[float] = []
    max_src = len(samples) - 1
    for idx in range(out_len):
        src_pos = idx * src_rate_hz / dst_rate_hz
        left = int(src_pos)
        right = min(left + 1, max_src)
        frac = src_pos - left
        value = samples[left] * (1.0 - frac) + samples[right] * frac
        resampled.append(value)
    return resampled


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Call orchestration-service with a WAV file."
    )
    parser.add_argument("--wav", required=True, help="Path to input .wav file")
    parser.add_argument(
        "--orchestrator",
        default="http://127.0.0.1:8090",
        help="Base orchestration URL (default: http://127.0.0.1:8090)",
    )
    parser.add_argument(
        "--mode",
        choices=("redub-wav", "transcribe"),
        default="redub-wav",
        help="Request mode (default: redub-wav)",
    )
    parser.add_argument(
        "--out",
        default="redub-output.wav",
        help="Output WAV path for redub-wav mode (default: redub-output.wav)",
    )
    parser.add_argument(
        "--language",
        default="en",
        help="Language hint (example: en, fr, auto)",
    )
    parser.add_argument(
        "--session-id",
        default=None,
        help="Optional session id (auto-generated if omitted)",
    )
    parser.add_argument(
        "--timeout",
        type=int,
        default=120,
        help="HTTP timeout in seconds (default: 120)",
    )
    parser.add_argument(
        "--max-samples",
        type=int,
        default=None,
        help="Optional cap on sent samples (useful for very large files)",
    )
    parser.add_argument(
        "--target-sample-rate",
        type=int,
        default=16000,
        help="Target sample rate expected by ASR service (default: 16000)",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    wav_path = Path(args.wav)
    if not wav_path.exists():
        print(f"File not found: {wav_path}", file=sys.stderr)
        return 1

    try:
        samples, src_rate_hz = read_wav_samples(wav_path)
    except Exception as exc:  # noqa: BLE001
        print(f"Failed to read WAV file: {exc}", file=sys.stderr)
        return 2

    if src_rate_hz != args.target_sample_rate:
        samples = resample_linear(samples, src_rate_hz, args.target_sample_rate)
        print(
            f"Resampled audio: {src_rate_hz} Hz -> {args.target_sample_rate} Hz "
            f"({len(samples)} samples)"
        )
    else:
        print(f"Audio sample rate: {src_rate_hz} Hz (no resampling)")

    if args.max_samples is not None and args.max_samples > 0:
        samples = samples[: args.max_samples]

    payload: dict[str, object] = {
        "samples": samples,
        "sample_rate_hz": args.target_sample_rate,
        "language_hint": args.language,
        "session_id": args.session_id or str(uuid.uuid4()),
    }
    print(f"Prepared payload with {len(samples)} samples")

    base_url = args.orchestrator.rstrip("/")
    endpoint = (
        f"{base_url}/api/asr/redub"
        if args.mode == "redub-wav"
        else f"{base_url}/api/asr/transcribe"
    )
    print(f"POST {endpoint}")

    try:
        response = requests.post(
            endpoint,
            json=payload,
            timeout=args.timeout,
        )
    except requests.RequestException as exc:
        print(f"Request failed: {exc}", file=sys.stderr)
        return 3

    print(f"HTTP {response.status_code}")

    if not response.ok:
        # Error responses are usually JSON, but we handle plain text too.
        try:
            print(response.json())
        except ValueError:
            print(response.text)
        return 4

    if args.mode == "redub-wav":
        out_path = Path(args.out)
        out_path.write_bytes(response.content)
        print(f"WAV saved: {out_path} ({len(response.content)} bytes)")
        return 0

    try:
        print(response.json())
    except ValueError:
        print(response.text)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
