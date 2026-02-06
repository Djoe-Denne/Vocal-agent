"""
TTS Server Configuration

Configuration dataclasses and loader for the Qwen3 TTS server.
Reads settings from tts.toml in the project root by default.
"""

from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

# tomllib is available in Python 3.11+, use tomli for older versions
try:
    import tomllib
except ImportError:  # pragma: no cover
    import tomli as tomllib


@dataclass
class ServerConfig:
    host: str = "127.0.0.1"
    port: int = 8001
    log_level: str = "info"
    preload_model: bool = False


@dataclass
class ModelConfig:
    model: str = "Qwen/Qwen3-TTS-12Hz-1.7B-Base"
    device_map: str = "cuda:0"
    torch_dtype: str = "bfloat16"
    language: str = "English"
    voices_dir: Path = Path("./tts_server/voices")


@dataclass
class OutputConfig:
    response_format: str = "wav"


@dataclass
class TTSConfig:
    server: ServerConfig = field(default_factory=ServerConfig)
    model: ModelConfig = field(default_factory=ModelConfig)
    output: OutputConfig = field(default_factory=OutputConfig)


def _load_toml(path: Path) -> dict:
    with path.open("rb") as f:
        return tomllib.load(f)


def _resolve_dir(base_dir: Path, value: str | Path) -> Path:
    path = Path(value)
    if not path.is_absolute():
        path = base_dir / path
    path.mkdir(parents=True, exist_ok=True)
    return path


def load_config(config_path: Optional[Path] = None) -> TTSConfig:
    """
    Load configuration from tts.toml.

    Args:
        config_path: Optional path to config file. If not provided,
                     looks for tts.toml in the parent directory of the package.
    """
    if config_path is None:
        config_path = Path(__file__).resolve().parent.parent / "tts.toml"

    if not config_path.exists():
        raise SystemExit(f"Config file not found: {config_path}")

    data = _load_toml(config_path)
    base_dir = config_path.parent

    server = ServerConfig(**data.get("server", {}))
    model_data = data.get("model", {})
    if "voices_dir" in model_data:
        model_data["voices_dir"] = _resolve_dir(base_dir, model_data["voices_dir"])
    model = ModelConfig(**model_data)
    if "voices_dir" not in model_data:
        model.voices_dir = _resolve_dir(base_dir, model.voices_dir)
    output = OutputConfig(**data.get("output", {}))

    return TTSConfig(server=server, model=model, output=output)
