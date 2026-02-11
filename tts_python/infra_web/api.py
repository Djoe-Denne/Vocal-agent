"""
FastAPI server exposing OpenAI-compatible /v1/audio/speech endpoint.

Supports optional pipeline-based processing via the ``pipeline`` body
field.  When no pipeline is configured or requested, falls back to the
original TTSService flow.
"""

from __future__ import annotations

import io
from typing import Optional, Tuple

import numpy as np
import soundfile as sf
from fastapi import FastAPI, HTTPException
from fastapi.responses import Response
from pydantic import BaseModel, Field

from tts_python.application.config import load_config
from tts_python.application.tts_service import TTSService, create_tts_model


# ---------------------------------------------------------------------------
# Request / response models
# ---------------------------------------------------------------------------

class SpeechRequest(BaseModel):
    input: str = Field(..., min_length=1)
    voice_sample: Optional[str] = Field(default=None, min_length=1)
    voice_preset: Optional[str] = Field(default=None, min_length=1)
    guidance: Optional[str] = Field(default=None)
    pipeline: Optional[str] = Field(
        default=None,
        description="Named pipeline from [pipelines.*] config (e.g. 'tts_default')",
    )


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

# Pipeline support (lazy — only built if config contains [pipelines])
_pipelines: Optional[dict] = None


def _get_pipelines() -> dict:
    """Build and cache pipelines from config on first access."""
    global _pipelines
    if _pipelines is None:
        if config.pipelines_raw:
            # Import stage modules so @register_stage decorators execute
            import tts_python.infra_pytorch.stage as _  # noqa: F401
            import shared.infra_stages.regex_cleanup as _  # noqa: F401
            import shared.infra_stages.text_normalize as _  # noqa: F401

            from shared.application.pipeline_factory import load_pipelines

            config_raw = {
                "pipelines": config.pipelines_raw,
                "stages": config.stages_raw,
            }
            _pipelines = load_pipelines(config_raw)
        else:
            _pipelines = {}
    return _pipelines


@app.on_event("startup")
def _startup() -> None:
    if config.server.preload_model:
        tts_model.load()
        # Also preload pipeline stages if pipelines are configured
        for pipe in _get_pipelines().values():
            pipe.load_all()


# ---------------------------------------------------------------------------
# Endpoints
# ---------------------------------------------------------------------------

@app.post("/v1/audio/speech")
def create_speech(request: SpeechRequest) -> Response:
    # ---- pipeline path ----------------------------------------------------
    if request.pipeline is not None:
        pipelines = _get_pipelines()
        if request.pipeline not in pipelines:
            raise HTTPException(
                status_code=400,
                detail=(
                    f"Unknown pipeline: {request.pipeline!r}. "
                    f"Available: {sorted(pipelines)}"
                ),
            )

        from shared.domain.pipeline import PipelineContext

        pipe = pipelines[request.pipeline]
        ctx = PipelineContext(text=request.input)
        # Forward voice / guidance info via meta so TTS stages can read them
        if request.voice_sample:
            from tts_python.application.tts_service import _resolve_voice_sample

            sample_path, sample_text = _resolve_voice_sample(
                config.model.voices_dir, request.voice_sample
            )
            ctx.meta["voice_sample_path"] = sample_path
            ctx.meta["voice_sample_text"] = sample_text
        if request.voice_preset:
            ctx.meta["voice_preset"] = request.voice_preset
        if request.guidance:
            ctx.meta["guidance"] = request.guidance

        ctx = pipe.run(ctx)

        if ctx.audio is None:
            raise HTTPException(status_code=500, detail="Pipeline produced no audio.")

        fmt = config.output.response_format
        try:
            data, media_type = _encode_audio(ctx.audio, ctx.sample_rate, fmt)
        except ValueError as exc:
            raise HTTPException(status_code=400, detail=str(exc)) from exc

        return Response(content=data, media_type=media_type)

    # ---- legacy path (no pipeline) ----------------------------------------
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
