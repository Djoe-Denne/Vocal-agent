"""
FastAPI server exposing OpenAI-compatible /v1/audio/speech endpoint.
"""

from __future__ import annotations

import io
from typing import Optional, Tuple

import numpy as np
import soundfile as sf
from fastapi import FastAPI, HTTPException
from fastapi.responses import Response
from pydantic import BaseModel, Field

from tts_server.application.config import load_config
from tts_server.application.tts_service import TTSService, create_tts_model


# ---------------------------------------------------------------------------
# Request / response models
# ---------------------------------------------------------------------------

class SpeechRequest(BaseModel):
    input: str = Field(..., min_length=1)
    voice_sample: Optional[str] = Field(default=None, min_length=1)
    voice_preset: Optional[str] = Field(default=None, min_length=1)
    guidance: Optional[str] = Field(default=None)


# ---------------------------------------------------------------------------
# Audio encoding helper
# ---------------------------------------------------------------------------

def _encode_audio(audio: np.ndarray, sr: int, fmt: str) -> Tuple[bytes, str]:
    if audio.ndim > 1:
        audio = audio[:, 0]

    fmt = fmt.lower()
    if fmt == "pcm":
        pcm = np.clip(audio, -1.0, 1.0)
        pcm = (pcm * 32767.0).astype("<i2")
        return pcm.tobytes(), "audio/pcm"

    if fmt in {"wav", "flac", "ogg"}:
        buffer = io.BytesIO()
        sf.write(buffer, audio, sr, format=fmt.upper())
        return buffer.getvalue(), f"audio/{fmt}"

    raise ValueError(f"Unsupported response_format: {fmt}")


# ---------------------------------------------------------------------------
# App bootstrap (uses factory — no hardcoded adapter)
# ---------------------------------------------------------------------------

config = load_config()
tts_model = create_tts_model(config)
tts_service = TTSService(config, tts_model)

app = FastAPI(title="TTS Server", version="1.0.0")


@app.on_event("startup")
def _startup() -> None:
    if config.server.preload_model:
        tts_model.load()


# ---------------------------------------------------------------------------
# Endpoints
# ---------------------------------------------------------------------------

@app.post("/v1/audio/speech")
def create_speech(request: SpeechRequest) -> Response:
    try:
        result = tts_service.synthesize(
            text=request.input,
            voice_sample=request.voice_sample,
            voice_preset=request.voice_preset,
            guidance=request.guidance,
        )
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
    except Exception as exc:  # pragma: no cover - runtime/model errors
        raise HTTPException(status_code=500, detail=str(exc)) from exc

    try:
        data, media_type = _encode_audio(result.audio, result.sample_rate, result.response_format)
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc

    return Response(content=data, media_type=media_type)


@app.get("/v1/audio/voices")
def list_voices() -> dict:
    return tts_service.list_voices()
