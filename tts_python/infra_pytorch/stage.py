"""
Pipeline stage wrapper for the Qwen TTS model adapter.
"""

from __future__ import annotations

from shared.application.pipeline_factory import register_stage
from shared.domain.pipeline import MediaType, PipelineContext, Stage


class QwenTTSStage(Stage):
    """TTS stage using Qwen3-TTS (text -> audio)."""

    @property
    def name(self) -> str:
        return "qwen_tts"

    @property
    def input_type(self) -> MediaType:
        return MediaType.TEXT

    @property
    def output_type(self) -> MediaType:
        return MediaType.AUDIO

    def __init__(
        self,
        model: str = "Qwen/Qwen3-TTS-12Hz-1.7B-Base",
        device_map: str = "cuda:0",
        torch_dtype: str = "bfloat16",
        language: str = "French",
        voices_dir: str = "./tts_python/voices",
        **_kwargs,
    ) -> None:
        self._model_str = model
        self._device_map = device_map
        self._torch_dtype = torch_dtype
        self._language = language
        self._voices_dir = voices_dir
        self._wrapper = None

    def _ensure_wrapper(self):
        if self._wrapper is None:
            from pathlib import Path

            from tts_python.application.config import ModelConfig
            from tts_python.infra_pytorch.qwen_model import QwenTTSWrapper

            cfg = ModelConfig(
                model=self._model_str,
                device_map=self._device_map,
                torch_dtype=self._torch_dtype,
                language=self._language,
                voices_dir=Path(self._voices_dir),
            )
            self._wrapper = QwenTTSWrapper(cfg)

    def load(self) -> None:
        self._ensure_wrapper()
        self._wrapper.load()

    def unload(self) -> None:
        if self._wrapper is not None:
            self._wrapper.unload()

    def process(self, ctx: PipelineContext) -> PipelineContext:
        self._ensure_wrapper()
        self._wrapper.load()  # no-op if already loaded

        voice_preset = ctx.meta.get("voice_preset")
        voice_sample_path = ctx.meta.get("voice_sample_path")
        voice_sample_text = ctx.meta.get("voice_sample_text")
        guidance = ctx.meta.get("guidance")

        audio, sr = self._wrapper.generate(
            text=ctx.text,
            voice_preset=voice_preset,
            voice_sample_path=voice_sample_path,
            voice_sample_text=voice_sample_text,
            guidance=guidance,
        )
        ctx.audio = audio
        ctx.sample_rate = sr
        return ctx


@register_stage("qwen_tts")
def _build_qwen_tts(cfg: dict) -> Stage:
    return QwenTTSStage(**cfg)
