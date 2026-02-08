# Arsouille-agent

GPU-accelerated **Automatic Speech Recognition (ASR)** and **Text-to-Speech (TTS)** in Rust — designed to replace the equivalent Python stack (`ptt`, `tts_server`, `shared`).

## Project layout

```
├── shared_rs/      # Common crate: pipeline engine, domain primitives
├── asr/            # ASR crate (Qwen3-ASR via the `aha` candle backend)
└── tts/            # TTS crate (Qwen3-TTS via `qwen3-tts-rs`, voice cloning)
```

`asr` and `tts` are **independent Cargo projects** (no workspace) — each has its
own `Cargo.lock` and dependency tree. This is required because `aha` needs
candle 0.9.1 while `qwen3-tts` needs candle 0.9.2, and these are
incompatible within a single Cargo resolution.

Each crate follows a **hexagonal (ports & adapters) architecture**.
See [`ARCHITECTURE.md`](ARCHITECTURE.md) for the full design documentation.

## Prerequisites

| Requirement | Notes |
|---|---|
| **Rust stable** (≥ 1.78) | `rustup update stable` |
| **CUDA toolkit** (≥ 12.x) | Both `aha` and `qwen3-tts` compile CUDA kernels |
| **Visual Studio 2022** Build Tools | `cl.exe` must be on `PATH` for `nvcc` |

On Windows you **must** load the MSVC environment before building:

```cmd
call "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"
```

## Building

Each crate is built independently from its own directory:

```cmd
cd asr && cargo build
cd tts && cargo build
```

For optimised builds:

```cmd
cd asr && cargo build --release
cd tts && cargo build --release
```

## Running

### ASR — transcribe an audio file

```cmd
cd asr
cargo run -- transcribe audio.wav
cargo run -- transcribe audio.wav --language en
cargo run -- transcribe audio.wav --output transcript.txt
cargo run -- --model-dir ./models/Qwen3-ASR-1.7b transcribe audio.wav
cargo run -- --config asr_config.toml transcribe audio.wav
```

### ASR — web API server

```cmd
cd asr
cargo run -- serve
cargo run -- serve --host 0.0.0.0 --port 3001
```

Once the server is running, send requests with `curl`:

```cmd
curl -X POST http://localhost:3001/transcribe -F "file=@audio.wav" -F "language=fr"
curl http://localhost:3001/health
```

Or PowerShell:

```powershell
$form = @{ file = Get-Item audio.wav; language = "fr" }
Invoke-RestMethod -Uri http://localhost:3001/transcribe -Method Post -Form $form
```

### TTS — synthesise speech

The model is downloaded automatically from HuggingFace on first run:

```cmd
cd tts
cargo run -- synthesize "Bonjour le monde"
cargo run -- synthesize "Hello world" --voice ryan --language english --output out.wav
cargo run -- --model-id Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice synthesize "Salut" --voice serena
```

To use a local model directory instead:

```cmd
cargo run -- --model-dir ./models/1.7b-base synthesize "Bonjour"
```

### TTS — web API server

```cmd
cd tts
cargo run -- serve
cargo run -- serve --host 0.0.0.0 --port 3002
```

Once the server is running, send requests with `curl`:

```cmd
curl -X POST http://localhost:3000/synthesize -H "Content-Type: application/json" ^
  -d "{\"text\": \"Bonjour le monde\", \"voice\": \"ryan\", \"language\": \"french\"}" ^
  --output output.wav
curl http://localhost:3000/health
```

Or PowerShell:

```powershell
$body = @{ text = "Bonjour le monde"; voice = "ryan"; language = "french" } | ConvertTo-Json
Invoke-RestMethod -Uri http://localhost:3000/synthesize -Method Post -ContentType "application/json" -Body $body -OutFile output.wav
```

The `POST /synthesize` endpoint returns WAV audio directly. Synthesis metadata
(timings, warnings) is available in the `X-TTS-Metadata` response header as JSON.

#### Voice cloning with voice profiles

Cloned voices are used exactly like preset speakers. Place a reference audio
file in `tts/voices/<name>/` and use it by name:

```
tts/voices/
  justamon/
    reference.wav           ← required: a short sample of the voice to clone
    transcript.txt          ← optional: transcript of the audio (enables ICL mode)
```

Then synthesise with the cloned voice — no extra flags needed:

```cmd
cargo run -- synthesize "Hello world" --voice justamon
```

This works the same as `--voice ryan`; the engine automatically detects that
`justamon` is a voice profile (not a preset) and uses voice cloning under the
hood. If `transcript.txt` is present, ICL mode is used for higher quality;
otherwise x-vector mode is used (faster, no transcript needed).

> **Note:** Voice cloning requires a **Base** model (e.g.
> `Qwen/Qwen3-TTS-12Hz-1.7B-Base`). CustomVoice models only support preset
> speakers.

#### Ad-hoc voice cloning (without a voice profile)

You can also pass reference audio directly via CLI flags:

```cmd
cargo run -- --model-id Qwen/Qwen3-TTS-12Hz-1.7B-Base synthesize "Clone this voice" \
  --ref-audio reference.wav --ref-text "transcript of reference audio"
```

#### Text-described voice design

VoiceDesign models accept a natural-language voice description:

```cmd
cargo run -- --model-id Qwen/Qwen3-TTS-12Hz-1.7B-VoiceDesign synthesize "Hello" \
  --instruct "A cheerful young female voice with high pitch"
```

## Configuration

Both binaries accept an optional `--config <path>` TOML file.
CLI flags override config-file values, which override built-in defaults.

<details>
<summary>Example ASR config (<code>asr_config.toml</code>)</summary>

```toml
[defaults]
language = "fr"

[engine]
device = "auto"
model_dir = "./models/Qwen3-ASR-1.7b"

[pipeline]
pre = []
post = []
```

Environment variables `ASR_DEVICE` and `ASR_MODEL_DIR` override the corresponding
config values.

</details>

<details>
<summary>Example TTS config (<code>tts_config.toml</code>)</summary>

```toml
[defaults]
voice = "ryan"
language = "english"

[defaults.model]
type = "huggingface"
repo = "Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice"

[engine]
device = "cuda"
dtype = "bf16"
model_cache_dir = "./models"
voices_dir = "./voices"

[pipeline]
pre = []
post = []

[models.fast_local]
type = "local"
path = "/models/qwen-fast"
```

</details>

### Available TTS models

| Model | HuggingFace ID | Size | Speaker Conditioning |
|---|---|---|---|
| 0.6B Base | `Qwen/Qwen3-TTS-12Hz-0.6B-Base` | 1.8 GB | Voice cloning from reference audio |
| 0.6B CustomVoice | `Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice` | 1.8 GB | 9 preset speakers |
| 1.7B Base | `Qwen/Qwen3-TTS-12Hz-1.7B-Base` | 3.9 GB | Voice cloning from reference audio |
| 1.7B CustomVoice | `Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice` | 3.9 GB | 9 preset speakers |
| 1.7B VoiceDesign | `Qwen/Qwen3-TTS-12Hz-1.7B-VoiceDesign` | 3.8 GB | Text-described voices |

Available preset speakers (CustomVoice models): `serena`, `vivian`, `unclefu`,
`ryan`, `aiden`, `onoanna`, `sohee`, `eric`, `dylan`.

Custom cloned voices can be added by creating a directory in `tts/voices/`
(see [Voice cloning with voice profiles](#voice-cloning-with-voice-profiles) above).

## Dependency notes

| Crate | Constraint | Reason |
|---|---|---|
| **asr** | `candle-core/nn/transformers = "=0.9.1"` | `aha`'s `match DType` is non-exhaustive for candle 0.9.2+ (`I16`, `I32`, `F8E4M3` variants) |
| **tts** | `qwen3-tts` latest (uses candle 0.9.2) | `sdpa()` API requires the 7-arg signature from candle-nn 0.9.2+ |

These two candle versions are **mutually incompatible**, which is why `asr` and
`tts` must be compiled as separate Cargo projects.

## Python stack (removed)

The original Python implementations (`ptt/`, `tts_server/`, `shared/`) have
been replaced by the Rust crates and removed from the repository.

## License

Private — © clawdbot
