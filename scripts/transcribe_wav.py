#!/usr/bin/env python3
"""
Send a WAV file to the ASR transcribe endpoint.

This script converts 16-bit PCM WAV samples to float32-like values in [-1.0, 1.0]
and posts them as JSON to /api/asr/transcribe.
"""

from __future__ import annotations

import argparse
import json
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
    parser = argparse.ArgumentParser(description="Transcribe a WAV file via ASR endpoint.")
    parser.add_argument("--wav", required=True, help="Path to input .wav file")
    parser.add_argument(
        "--endpoint",
        default="http://127.0.0.1:8080/api/asr/transcribe",
        help="Transcribe endpoint URL",
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

    payload = {
        "samples": samples,
        "sample_rate_hz": args.target_sample_rate,
        "language_hint": args.language,
        "session_id": args.session_id or str(uuid.uuid4()),
    }

    payload_json = json.dumps(payload, ensure_ascii=True, separators=(",", ":"))
    payload_size_mb = len(payload_json.encode("utf-8")) / (1024 * 1024)
    print(
        f"Prepared payload: {len(samples)} samples, "
        f"{payload_size_mb:.2f} MB JSON body"
    )

    try:
        response = requests.post(
            args.endpoint,
            data=payload_json,
            headers={"Content-Type": "application/json"},
            timeout=args.timeout,
        )
    except requests.RequestException as exc:
        print(f"Request failed: {exc}", file=sys.stderr)
        return 3

    print(f"HTTP {response.status_code}")
    try:
        print(json.dumps(response.json(), indent=2, ensure_ascii=True))
    except ValueError:
        print(response.text)

    if (
        response.status_code == 400
        and "request_body_error" in response.text
        and payload_size_mb > 2.0
    ):
        print(
            "Hint: request body is large. Reduce --max-samples or increase "
            "server body limit.",
            file=sys.stderr,
        )

    return 0 if response.ok else 4


if __name__ == "__main__":
    raise SystemExit(main())
