# Court Redemis (Short README)

## What is this?

`asr-service` is a RustyCog-based ASR HTTP service.

Current API:
- `GET /health`
- `POST /api/asr/transcribe`

---

## Dependencies

Required:
- Rust (stable, edition 2021 compatible)
- Cargo
- `cmake` (needed by RustyCog/test-related native deps)

Windows recommended:
- Visual Studio Build Tools 2022 (C++ workload)

Project dependency (mandatory):
- `AIForAll/rustycog` must exist at this relative path from this repo:
  - `../../AIForAll/rustycog`

Optional runtime asset:
- Whisper model file (example): `models/ggml-small.bin`

---

## Installation

1. Ensure this folder layout exists:
   - `.../vocaloid/asr-service`
   - `.../AIForAll/rustycog`

2. From service root:

```powershell
cargo check
```

3. (Optional) Run tests:

```powershell
$env:RUN_ENV="test"
$env:CMAKE_GENERATOR="Visual Studio 17 2022"
cargo test --workspace
```

---

## How to use

### 1) Run in development

```powershell
$env:RUN_ENV="development"
cargo run -p asr-service
```

For real Whisper transcription (not fallback), run with feature:

```powershell
$env:RUN_ENV="development"
cargo run -p asr-service --features whisper-runtime
```

If you run without `--features whisper-runtime`, the service replies with
`"whisper-runtime feature disabled"` placeholder text.

### Whisper backend selection (CPU / GPU)

Default runtime feature (`whisper-runtime`) uses CPU backend unless a GPU feature is enabled.

CPU (default):

```powershell
$env:RUN_ENV="development"
cargo run -p asr-service --features whisper-runtime
```

NVIDIA CUDA:

```powershell
$env:RUN_ENV="development"
# Ensure CUDA toolkit is installed and CUDA_PATH is set
cargo run -p asr-service --features whisper-cuda
```

Vulkan (Windows/Linux):

```powershell
$env:RUN_ENV="development"
# Ensure Vulkan SDK is installed and VULKAN_SDK is set
cargo run -p asr-service --features whisper-vulkan
```

Note: this repository pins `CMAKE_GENERATOR` in `.cargo/config.toml` to avoid
Windows generator autodetection failures when compiling whisper bindings.

If you use DTW token timestamps, `service.asr.dtw_mem_size` in config is treated
as MiB for small values (for example `128` = 128 MiB).

### Pipeline configuration (plugin style)

`transcribe` now executes a config-defined pipeline:
- `pre` steps
- one `transcription` step
- `post` steps

Example (from `config/development.toml`):

```toml
[service.pipeline]
selected = "development"

[service.pipeline.definitions.development]
pre = ["resample", "audio_clamp"]
transcription = "whisper_transcription"
post = ["forced_alignment"]

[service.pipeline.plugins.resample]
enabled = true
target_sample_rate_hz = 16000
```

Built-in step plugins:
- `audio_clamp`
- `resample`
- `whisper_transcription`
- `forced_alignment`

### 2) Check health

```powershell
Invoke-WebRequest -Uri "http://127.0.0.1:8080/health" -UseBasicParsing
```

### 3) Transcribe audio samples

```powershell
$body = @{
  samples = @(0.0, 0.1, 0.2, 0.3)
  sample_rate_hz = 16000
  language_hint = "en"
  session_id = "demo-session"
} | ConvertTo-Json

Invoke-RestMethod `
  -Method Post `
  -Uri "http://127.0.0.1:8080/api/asr/transcribe" `
  -ContentType "application/json" `
  -Body $body
```

Response includes:
- `session_id`
- `transcript`
- `aligned_words`
- `text`

### 4) Python script (WAV file -> transcribe endpoint)

If you usually work with Python and `.wav` files, use:
- `scripts/transcribe_wav.py`

Install Python dependency:

```powershell
pip install requests
```

Run:

```powershell
python scripts/transcribe_wav.py `
  --wav "C:\path\to\audio.wav" `
  --endpoint "http://127.0.0.1:8080/api/asr/transcribe" `
  --language "en"
```

Notes:
- The script currently supports **16-bit PCM WAV**.
- For multi-channel WAV, it uses the **first channel**.
- By default it resamples to **16 kHz** and sends `sample_rate_hz` in the payload.
- For very large files, you can limit request size:

```powershell
python scripts/transcribe_wav.py --wav "C:\path\to\audio.wav" --max-samples 160000
```

Override target sample rate if needed:

```powershell
python scripts/transcribe_wav.py --wav "C:\path\to\audio.wav" --target-sample-rate 16000
```
