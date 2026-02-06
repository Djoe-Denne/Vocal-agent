"""
FastAPI server for PTT byte-stream transcription.
"""

from __future__ import annotations

import io
from typing import Optional

import numpy as np
import soundfile as sf
from fastapi import FastAPI, HTTPException, Request
from pydantic import BaseModel

from ptt.application.config import load_config
from ptt.infra_pytorch.transcriber import create_transcriber


class TranscriptionResponse(BaseModel):
    text: str


config = load_config()
transcriber = create_transcriber(config)
app = FastAPI(title="PTT Transcription API", version="1.0.0")


@app.on_event("startup")
def _startup() -> None:
    if config.api.preload_model:
        transcriber.load_model()


@app.post("/v1/audio/transcriptions", response_model=TranscriptionResponse)
async def transcribe_audio(request: Request) -> TranscriptionResponse:
    try:
        payload = await request.body()
    except Exception as exc:
        raise HTTPException(status_code=400, detail=f"Failed to read body: {exc}") from exc

    if not payload:
        raise HTTPException(status_code=400, detail="Empty request body.")

    try:
        audio_data, sample_rate = sf.read(io.BytesIO(payload), dtype="float32")
    except Exception as exc:
        raise HTTPException(
            status_code=400,
            detail=f"Unsupported audio stream. Provide WAV/FLAC/OGG bytes. Error: {exc}",
        ) from exc

    if isinstance(audio_data, np.ndarray) and audio_data.ndim > 1:
        audio_data = audio_data.mean(axis=1)

    transcriber.load_model()
    result = transcriber.transcribe_array(audio_data, int(sample_rate))
    if not result or not result.text:
        return TranscriptionResponse(text="")
    return TranscriptionResponse(text=result.text)
