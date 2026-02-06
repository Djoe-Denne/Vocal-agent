"""
Shim for Qwen3 TTS model wrapper (moved to infra_pytorch).
"""

from tts_server.infra_pytorch.qwen_model import QwenTTSWrapper

__all__ = ["QwenTTSWrapper"]
