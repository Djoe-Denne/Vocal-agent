# Architecture Guide

This document describes the architecture shared by the **PTT** (Push-to-Talk / Speech-to-Text) and **TTS** (Text-to-Speech) modules, and explains how to extend them with new behaviours or model backends.

---

## Design Pattern — Ports & Adapters (Hexagonal Architecture)

Both modules follow the same three-layer structure:

```
module/
  domain/          ← pure contracts + data models (no framework deps)
    ports.py       ← abstract base classes (the "ports")
    models.py      ← dataclasses consumed by every layer
  application/     ← orchestration, config, factory wiring
    config.py      ← configuration dataclasses + TOML loader
    service.py     ← application service that coordinates ports
    factories.py   ← factory functions that pick the right adapter
  infra_<name>/    ← one directory per swappable adapter
    <impl>.py      ← concrete class implementing a port
```

### Key principles

| Principle | How it's applied |
|---|---|
| **Dependency inversion** | Domain ports define interfaces. Infrastructure adapters implement them. No domain code imports infra code. |
| **Application decides** | The `application/` layer reads config and calls a **factory** that selects the right adapter at runtime. |
| **One port, N adapters** | Each capability (transcription, TTS, reconciliation, recording, …) has exactly one abstract port and one or more concrete adapters. |
| **Lazy imports** | Factories use local `from … import` so heavy dependencies (torch, whisper, qwen_tts, …) are only loaded when the adapter is actually chosen. |

---

## Module Layout

### TTS (`tts_server/`)

```
tts_server/
  __main__.py                      ← python -m tts_server
  domain/
    ports.py                       ← TTSModel (ABC)
    models.py                      ← SynthesisResult
  application/
    config.py                      ← TTSConfig, load_config()
    tts_service.py                 ← TTSService + create_tts_model() factory
  infra_pytorch/
    qwen_model.py                  ← QwenTTSWrapper(TTSModel)
  infra_web/
    api.py                         ← FastAPI /v1/audio/speech endpoint
  infra_cli/
    cli.py                         ← uvicorn runner
  voices/                          ← voice sample presets (audio.wav + text.txt)
```

**Port** → `TTSModel`
**Adapters** → `QwenTTSWrapper` (add more in `infra_<name>/`)

### PTT (`ptt/`)

```
ptt/
  __main__.py                      ← python -m ptt --daemon | --api | --file
  domain/
    ports.py                       ← BaseTranscriber (ABC), BaseReconciler (ABC)
    models.py                      ← TranscriptionResult, ReconciliationResult, AudioChunk
  application/
    config.py                      ← Config, load_config()
    factories.py                   ← create_transcriber(), create_reconciler()
    ptt_app.py                     ← PTTApplication orchestrator
  infra_whisper/
    transcriber.py                 ← WhisperTranscriber(BaseTranscriber)
  infra_huggingface/
    transcriber.py                 ← HuggingFaceTranscriber(BaseTranscriber)
  infra_reconcilers/
    word_overlap.py, fuzzy.py,     ← reconciler adapters
    llm.py, none.py
  infra_sounddevice/
    recorder.py                    ← StreamingRecorder
  infra_pynput/
    hotkeys.py                     ← HotkeyManager
  infra_podman/
    openclaw.py                    ← OpenClawClient
  infra_daemon/
    daemon.py                      ← PTTDaemon (hotkey + remote API)
  infra_web/
    api.py                         ← FastAPI /v1/audio/transcriptions
    api_client.py                  ← HTTP client for remote transcription
    cli.py                         ← uvicorn runner
  utils/
    audio.py                       ← prepare_audio(), play_beep()
    logging.py                     ← shared logger setup
```

**Ports** → `BaseTranscriber`, `BaseReconciler`
**Adapters** → Whisper, HuggingFace (transcribers) · WordOverlap, Fuzzy, LLM, NoOp (reconcilers)

---

## Data Flow

### TTS request flow

```
HTTP request
  → infra_web/api.py (FastAPI)
    → application/tts_service.py (TTSService.synthesize)
      → domain/ports.py (TTSModel.generate)  ← abstract call
        → infra_pytorch/qwen_model.py        ← concrete adapter
      ← (np.ndarray, sample_rate)
    ← SynthesisResult
  ← WAV / FLAC / OGG bytes
```

### PTT push-to-talk flow

```
Hotkey press
  → infra_pynput/hotkeys.py (HotkeyManager)
    → application/ptt_app.py (PTTApplication._start_recording)
      → infra_sounddevice/recorder.py (StreamingRecorder)
        ↓ audio chunks
      → domain/ports.py (BaseTranscriber.transcribe_array)  ← abstract
        → infra_whisper/ or infra_huggingface/               ← concrete
      → domain/ports.py (BaseReconciler.add_segment)         ← abstract
        → infra_reconcilers/*                                 ← concrete
      ← reconciled text
    → infra_podman/openclaw.py (send to AI agent)
```

---

## How to Add a New TTS Model

### 1. Create an adapter directory

```
tts_server/infra_bark/
  __init__.py
  bark_model.py
```

### 2. Implement the `TTSModel` port

```python
# tts_server/infra_bark/bark_model.py

from tts_server.domain.ports import TTSModel
from tts_server.application.config import ModelConfig

class BarkTTSWrapper(TTSModel):
    def __init__(self, config: ModelConfig) -> None:
        self._config = config
        self._model = None

    def load(self) -> None:
        # Load the Bark model
        ...

    def unload(self) -> None:
        # Free GPU memory
        ...

    def generate(
        self,
        text,
        voice_preset=None,
        voice_sample_path=None,
        voice_sample_text=None,
        guidance=None,
    ):
        # Generate audio → return (np.ndarray, sample_rate)
        ...

    def get_supported_speakers(self):
        return ["v2/en_speaker_0", "v2/en_speaker_1", ...]

    def get_supported_languages(self):
        return ["en", "fr", "de", ...]
```

### 3. Register it in the factory

```python
# tts_server/application/tts_service.py  →  create_tts_model()

def create_tts_model(config: TTSConfig) -> TTSModel:
    backend = config.model.model.lower()

    if "bark" in backend:
        from tts_server.infra_bark.bark_model import BarkTTSWrapper
        return BarkTTSWrapper(config.model)

    # default: Qwen
    from tts_server.infra_pytorch.qwen_model import QwenTTSWrapper
    return QwenTTSWrapper(config.model)
```

No other files need to change — the API layer, service, and domain port are untouched.

---

## How to Add a New ASR Transcriber

### 1. Create an adapter directory

```
ptt/infra_faster_whisper/
  __init__.py
  transcriber.py
```

### 2. Implement the `BaseTranscriber` port

```python
# ptt/infra_faster_whisper/transcriber.py

from ptt.domain.ports import BaseTranscriber
from ptt.domain.models import TranscriptionResult
from ptt.application.config import Config

class FasterWhisperTranscriber(BaseTranscriber):
    def __init__(self, config: Config) -> None:
        self._config = config
        self._model = None

    @property
    def is_loaded(self) -> bool:
        return self._model is not None

    def load_model(self) -> bool:
        from faster_whisper import WhisperModel
        self._model = WhisperModel(self._config.asr.whisper.model)
        return True

    def unload_model(self) -> bool:
        self._model = None
        return True

    def transcribe_file(self, audio_path):
        segments, info = self._model.transcribe(str(audio_path))
        text = " ".join(seg.text for seg in segments)
        return TranscriptionResult(text=text, segments=[], duration=0, audio_duration=info.duration)

    def transcribe_array(self, audio_data, sample_rate, on_segment=None):
        # Use ptt.utils.audio.prepare_audio() to convert to 16 kHz float32
        from ptt.utils.audio import prepare_audio
        audio = prepare_audio(audio_data, sample_rate)
        ...
```

### 3. Register it in the factory

```python
# ptt/application/factories.py  →  create_transcriber()

def create_transcriber(config: Config) -> BaseTranscriber:
    backend = config.asr.backend.lower()

    if backend == "faster_whisper":
        from ptt.infra_faster_whisper.transcriber import FasterWhisperTranscriber
        return FasterWhisperTranscriber(config)
    ...
```

### 4. (Optional) Add config fields

Add any new settings to the relevant dataclass in `ptt/application/config.py` and parse them in `load_config()`.

---

## How to Add a New Reconciler

Same pattern — implement `BaseReconciler.reconcile()` and register in `create_reconciler()`:

```python
# ptt/infra_reconcilers/my_algo.py

from ptt.domain.ports import BaseReconciler
from ptt.domain.models import ReconciliationResult

class MyAlgoReconciler(BaseReconciler):
    def reconcile(self, previous_text: str, current_text: str) -> ReconciliationResult:
        # Your overlap-detection / merging logic here
        ...
```

Then add a branch in `ptt/application/factories.py → create_reconciler()`.

---

## How to Add a New Behaviour (e.g. a new I/O adapter)

For non-model infrastructure (audio input, hotkeys, network clients, …):

1. **Create `ptt/infra_<name>/`** with your implementation.
2. **If the behaviour is substitutable** (e.g. a different audio backend), define or extend a port in `domain/ports.py` and wire a factory.
3. **If it's additive** (e.g. a new output target), just import and use it in the application layer (`ptt_app.py` or `daemon.py`).

---

## Configuration

Both modules use TOML config files in the project root:

| Module | Config file | Loader |
|--------|-------------|--------|
| TTS | `tts.toml` | `tts_server.application.config.load_config()` |
| PTT | `ptt.toml` | `ptt.application.config.load_config()` |

Configuration is parsed into typed dataclasses so new fields are self-documenting and IDE-friendly.

---

## Running

```bash
# TTS server
python -m tts_server                 # starts the FastAPI server

# PTT interactive (push-to-talk with streaming)
python ptt.py                        # default: local model + hotkeys

# PTT daemon (hotkey recording → remote API transcription)
python -m ptt --daemon

# PTT API server
python -m ptt --api

# PTT file transcription
python -m ptt --file path/to/audio.wav
```
