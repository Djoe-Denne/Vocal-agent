"""
PTT Configuration

Configuration dataclasses and loader for the PTT application.
Reads settings from ptt.toml in the project root by default.
"""

import os
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

try:
    import tomllib
except ImportError:
    import tomli as tomllib


@dataclass
class WhisperConfig:
    """Configuration specific to OpenAI Whisper backend."""
    model: str = "large"
    language: str = "fr"
    force_fp32: bool = False
    initial_prompt: str = ""
    suppress_fillers: bool = False


@dataclass
class HuggingFaceConfig:
    """Configuration specific to HuggingFace transformers backend."""
    model: str = "Qwen/Qwen3-ASR-1.7B"
    language: str = "fr"
    torch_dtype: str = "float16"
    device_map_auto: bool = True


@dataclass
class ASRConfig:
    """Configuration for Automatic Speech Recognition."""
    backend: str = "huggingface"
    save_transcription: bool = False
    whisper: WhisperConfig = field(default_factory=WhisperConfig)
    huggingface: HuggingFaceConfig = field(default_factory=HuggingFaceConfig)


@dataclass
class ReconcilerConfig:
    """Configuration for text reconciliation."""
    algorithm: str = "word_overlap"
    min_overlap_words: int = 3
    max_context_words: int = 15
    fuzzy_threshold: float = 0.8
    llm_model: str = "HuggingFaceTB/SmolLM2-360M-Instruct"
    llm_device: str = "cuda"
    llm_cleanup_hesitations: bool = True


@dataclass
class StreamingConfig:
    """Configuration for streaming transcription."""
    chunk_duration: float = 5.0
    overlap_duration: float = 1.0


@dataclass
class DaemonConfig:
    """Configuration for daemon API mode."""
    api_url: str = ""
    api_timeout: int = 60
    api_headers: dict[str, str] = field(default_factory=dict)
    api_response_key: str = "text"
    api_file_field: str = "file"
    api_model: str = ""
    api_language: str = ""
    api_extra_fields: dict[str, str] = field(default_factory=dict)
    delete_audio_after_send: bool = True


@dataclass
class ApiConfig:
    """Configuration for HTTP API server."""
    host: str = "127.0.0.1"
    port: int = 8002
    log_level: str = "info"
    preload_model: bool = False


@dataclass
class Config:
    """Main configuration for PTT application."""
    # Hotkeys
    hotkey_toggle: str
    hotkey_attach_image: str

    # Audio
    tmp_dir: Path
    rate: int
    channels: int

    # Beep
    beep_start_stop: bool
    beep_every: int
    beep_frequency: int
    beep_duration: int

    # Clipboard
    clipboard_prefix: str
    clipboard_delete_after_send: bool

    # OpenClaw
    openclaw_send: bool
    openclaw_container_name: str
    openclaw_session_id: str
    openclaw_single_line: bool
    openclaw_max_chars: int
    openclaw_shared_dir: Path

    # Optional fields with defaults
    hotkey_unload_model: str = "<ctrl>+<alt>+u"

    # Sub-configurations
    asr: ASRConfig = field(default_factory=ASRConfig)
    streaming: StreamingConfig = field(default_factory=StreamingConfig)
    reconciler: ReconcilerConfig = field(default_factory=ReconcilerConfig)
    daemon: DaemonConfig = field(default_factory=DaemonConfig)
    api: ApiConfig = field(default_factory=ApiConfig)


def _resolve_shared_dir(script_dir: Path, shared_dir_str: str) -> Path:
    """Resolve and create the shared directory."""
    if shared_dir_str.startswith("./") or shared_dir_str.startswith(".\\"):
        shared_dir = script_dir / shared_dir_str[2:]
    else:
        shared_dir = Path(os.path.expandvars(shared_dir_str))
    shared_dir.mkdir(parents=True, exist_ok=True)
    return shared_dir


def load_config(config_path: Optional[Path] = None) -> Config:
    """
    Load configuration from ptt.toml.

    Args:
        config_path: Optional path to config file. If not provided,
                     looks for ptt.toml in the parent directory of the ptt package.
    """
    if config_path is None:
        package_dir = Path(__file__).resolve().parent.parent  # ptt/
        script_dir = package_dir.parent                       # transcrption/
        config_path = script_dir / "ptt.toml"
    else:
        script_dir = config_path.parent

    if not config_path.exists():
        print(f"[ERR] Config file not found: {config_path}", flush=True)
        sys.exit(1)

    try:
        d = tomllib.loads(config_path.read_text(encoding="utf-8"))
    except Exception as e:
        print(f"[ERR] Failed to parse config file: {e}", flush=True)
        sys.exit(1)

    # tmp_dir
    tmp_dir_str = d["audio"].get("tmp_dir", "")
    if tmp_dir_str:
        tmp_dir = Path(os.path.expandvars(tmp_dir_str))
    else:
        tmp_dir = Path(os.environ.get("TEMP", os.environ.get("TMP", ".")))
    tmp_dir.mkdir(parents=True, exist_ok=True)

    # streaming
    streaming_d = d.get("streaming", {})
    streaming = StreamingConfig(
        chunk_duration=streaming_d.get("chunk_duration", 5.0),
        overlap_duration=streaming_d.get("overlap_duration", 1.0),
    )

    # reconciler
    reconciler_d = d.get("reconciler", {})
    reconciler = ReconcilerConfig(
        algorithm=reconciler_d.get("algorithm", "word_overlap"),
        min_overlap_words=reconciler_d.get("min_overlap_words", 3),
        max_context_words=reconciler_d.get("max_context_words", 15),
        fuzzy_threshold=reconciler_d.get("fuzzy_threshold", 0.8),
        llm_model=reconciler_d.get("llm_model", "HuggingFaceTB/SmolLM2-360M-Instruct"),
        llm_device=reconciler_d.get("llm_device", "cuda"),
        llm_cleanup_hesitations=reconciler_d.get("llm_cleanup_hesitations", True),
    )

    # daemon
    daemon_d = d.get("daemon", {})
    daemon = DaemonConfig(
        api_url=daemon_d.get("api_url", ""),
        api_timeout=daemon_d.get("api_timeout", 60),
        api_headers=daemon_d.get("api_headers", {}),
        api_response_key=daemon_d.get("api_response_key", "text"),
        api_file_field=daemon_d.get("api_file_field", "file"),
        api_model=daemon_d.get("api_model", ""),
        api_language=daemon_d.get("api_language", ""),
        api_extra_fields=daemon_d.get("api_extra_fields", {}),
        delete_audio_after_send=daemon_d.get("delete_audio_after_send", True),
    )

    # api
    api_d = d.get("api", {})
    api = ApiConfig(
        host=api_d.get("host", "127.0.0.1"),
        port=api_d.get("port", 8002),
        log_level=api_d.get("log_level", "info"),
        preload_model=api_d.get("preload_model", False),
    )

    # ASR (with backward compatibility for old [whisper] section)
    asr_d = d.get("asr", {})
    old_whisper_d = d.get("whisper", {})

    whisper_d = asr_d.get("whisper", {})
    whisper_config = WhisperConfig(
        model=whisper_d.get("model", old_whisper_d.get("model", "large")),
        language=whisper_d.get("language", old_whisper_d.get("language", "fr")),
        force_fp32=whisper_d.get("force_fp32", old_whisper_d.get("force_fp16_false", False)),
        initial_prompt=whisper_d.get("initial_prompt", old_whisper_d.get("initial_prompt", "")),
        suppress_fillers=whisper_d.get("suppress_fillers", old_whisper_d.get("suppress_fillers", False)),
    )

    hf_d = asr_d.get("huggingface", {})
    hf_config = HuggingFaceConfig(
        model=hf_d.get("model", "Qwen/Qwen3-ASR-1.7B"),
        language=hf_d.get("language", "fr"),
        torch_dtype=hf_d.get("torch_dtype", "float16"),
        device_map_auto=hf_d.get("device_map_auto", True),
    )

    asr = ASRConfig(
        backend=asr_d.get("backend", "huggingface"),
        save_transcription=asr_d.get("save_transcription", old_whisper_d.get("save_transcription", False)),
        whisper=whisper_config,
        huggingface=hf_config,
    )

    cfg = Config(
        hotkey_toggle=d["hotkey"]["toggle"],
        hotkey_attach_image=d["hotkey"]["attach_clipboard_image"],
        hotkey_unload_model=d["hotkey"].get("unload_model", "<ctrl>+<alt>+u"),
        tmp_dir=tmp_dir,
        rate=d["audio"]["rate"],
        channels=d["audio"]["channels"],
        beep_start_stop=d["beep"]["start_stop"],
        beep_every=d["beep"]["every_seconds"],
        beep_frequency=d["beep"].get("frequency", 800),
        beep_duration=d["beep"].get("duration_ms", 200),
        clipboard_prefix=d["clipboard"]["prefix"],
        clipboard_delete_after_send=d["clipboard"]["delete_after_send"],
        openclaw_send=d["openclaw"]["send"],
        openclaw_container_name=d["openclaw"]["container_name"],
        openclaw_session_id=d["openclaw"]["session_id"],
        openclaw_single_line=d["openclaw"]["single_line"],
        openclaw_max_chars=d["openclaw"]["max_chars"],
        openclaw_shared_dir=_resolve_shared_dir(script_dir, d["openclaw"].get("shared_dir", "./shared")),
        asr=asr,
        streaming=streaming,
        reconciler=reconciler,
        daemon=daemon,
        api=api,
    )

    return cfg


__all__ = [
    "WhisperConfig",
    "HuggingFaceConfig",
    "ASRConfig",
    "ReconcilerConfig",
    "StreamingConfig",
    "DaemonConfig",
    "ApiConfig",
    "Config",
    "load_config",
]
