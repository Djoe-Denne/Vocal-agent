# ASR Service

A modular Automatic Speech Recognition HTTP service built in Rust with a plugin-based pipeline architecture.

**API endpoints:**
- `GET /health`
- `POST /api/asr/transcribe`

---

## Architecture

```
asr-service          (binary entry point, feature flags)
├── setup            (pipeline loader, plugin registration)
├── application      (pipeline engine, step abstraction)
├── configuration    (config structs, TOML loading)
├── domain           (traits: TranscriptionPort, AlignmentPort, PipelineStage)
├── http             (Axum HTTP handlers)
├── infra-audio      (audio preprocessing: clamp, resample)
├── infra-asr-whisper (Whisper transcription adapter + pipeline stage)
└── infra-alignment  (Wav2Vec2 CTC forced aligner + pipeline stage)
```

Each runtime capability is its own crate, gated behind a Cargo feature flag.
Nothing compiles unless you opt in.

---

## Prerequisites

| Requirement | Notes |
|---|---|
| Rust stable (edition 2021) | `rustup update stable` |
| Cargo | Ships with Rust |
| `cmake` | Needed to compile whisper-rs native bindings |
| Visual Studio Build Tools 2022 | Windows only -- C++ workload |
| `AIForAll/rustycog` at `../../AIForAll/rustycog` | Shared framework crates |

---

## Quick start

### 1. Check the build (no ML runtimes)

```powershell
cargo check
```

This compiles everything except the optional ML backends.
Useful to verify the toolchain and project layout.

### 2. Run with Whisper transcription (CPU)

```powershell
$env:RUN_ENV="development"
cargo run -p asr-service --features whisper-runtime
```

Requires a Whisper GGML model file at the path set in your config
(default: `models/ggml-large-v3-q5_0.bin`).

### 3. Run with Wav2Vec2 forced alignment

```powershell
$env:RUN_ENV="development"
cargo run -p asr-service --features whisper-runtime,wav2vec2-runtime
```

Requires three files for the Wav2Vec2 model (defaults shown):

| File | Default path | Source |
|---|---|---|
| Safetensors weights | `models/wav2vec2-fr.safetensors` | [bofenghuang/asr-wav2vec2-ctc-french](https://huggingface.co/bofenghuang/asr-wav2vec2-ctc-french) |
| Config | `models/wav2vec2-config.json` | Same repo, `config.json` |
| Vocabulary | `models/wav2vec2-vocab.json` | Same repo, `vocab.json` |

Override paths in your TOML config:

```toml
[service.pipeline.plugins.wav2vec2]
model_path  = "models/wav2vec2-fr.safetensors"
config_path = "models/wav2vec2-config.json"
vocab_path  = "models/wav2vec2-vocab.json"
device      = "cpu"   # or "cuda"
```

---

## Feature flags

Combine features as needed with `--features flag1,flag2`.

| Feature | What it enables |
|---|---|
| `whisper-runtime` | Whisper transcription (CPU) |
| `whisper-cuda` | Whisper + NVIDIA CUDA backend |
| `whisper-vulkan` | Whisper + Vulkan backend |
| `whisper-openblas` | Whisper + OpenBLAS backend |
| `wav2vec2-runtime` | Wav2Vec2 CTC forced alignment (CPU) |
| `wav2vec2-cuda` | Wav2Vec2 + CUDA tensor backend |

Without any features the service compiles but returns placeholder responses.

---

## Pipeline configuration

The `/api/asr/transcribe` endpoint executes a config-driven pipeline:

1. **pre** steps -- audio preprocessing
2. **transcription** step -- speech-to-text
3. **post** steps -- post-processing (alignment, etc.)

### Example (`config/development.toml`)

```toml
[service.pipeline]
selected = "development"

[service.pipeline.definitions.development]
pre = ["resample", "audio_clamp"]
transcription = "whisper_transcription"
post = ["wav2vec2_alignment"]

[service.pipeline.plugins.resample]
enabled = true
target_sample_rate_hz = 16000
```

### Available pipeline plugins

| Plugin name | Feature required | Crate |
|---|---|---|
| `audio_clamp` | *(always available)* | `infra-audio` |
| `resample` | *(always available)* | `infra-audio` |
| `whisper_transcription` | `whisper-runtime` | `infra-asr-whisper` |
| `wav2vec2_alignment` | `wav2vec2-runtime` | `infra-alignment` |

---

## Usage examples

### Health check

```powershell
Invoke-WebRequest -Uri "http://127.0.0.1:8080/health" -UseBasicParsing
```

### Transcribe audio samples (inline)

```powershell
$body = @{
  samples = @(0.0, 0.1, 0.2, 0.3)
  sample_rate_hz = 16000
  language_hint = "fr"
  session_id = "demo-session"
} | ConvertTo-Json

Invoke-RestMethod `
  -Method Post `
  -Uri "http://127.0.0.1:8080/api/asr/transcribe" `
  -ContentType "application/json" `
  -Body $body
```

Response includes `session_id`, `transcript`, `aligned_words`, and `text`.

### Transcribe a WAV file (Python helper)

```powershell
pip install requests
python scripts/transcribe_wav.py `
  --wav "C:\path\to\audio.wav" `
  --endpoint "http://127.0.0.1:8080/api/asr/transcribe" `
  --language "fr"
```

The script supports 16-bit PCM WAV, extracts the first channel from
multi-channel files, and resamples to 16 kHz by default.

Options:

```powershell
# Limit sample count for large files
python scripts/transcribe_wav.py --wav audio.wav --max-samples 160000

# Override target sample rate
python scripts/transcribe_wav.py --wav audio.wav --target-sample-rate 16000
```

---

## Tests

```powershell
$env:RUN_ENV="test"
$env:CMAKE_GENERATOR="Visual Studio 17 2022"
cargo test --workspace
```

---

## Notes

- `CMAKE_GENERATOR` is pinned in `.cargo/config.toml` to avoid Windows
  generator autodetection failures when compiling whisper-rs bindings.
- DTW token timestamps: `service.asr.dtw_mem_size` in config is treated as
  MiB for small values (e.g. `128` = 128 MiB).
- Wav2Vec2 alignment expects 16 kHz mono audio. Use the `resample` pre-step
  if your input may arrive at a different sample rate.
