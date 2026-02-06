# Push-to-Talk Speech-to-Text for Windows

A Windows push-to-talk application with **streaming transcription** that records audio, transcribes it in real-time, and sends the transcription to an AI agent (OpenClaw) running in a Podman container.

## Features

- **Multi-Backend ASR**: Choose between OpenAI Whisper or HuggingFace models (Qwen3-ASR, etc.)
- **Streaming Transcription**: Real-time transcription as you speak (configurable chunk size)
- **Push-to-Talk Recording**: Press a hotkey to start/stop recording
- **GPU Accelerated**: Full CUDA support
- **Modular Reconciliation**: Multiple algorithms for merging overlapping text segments
- **Clipboard Image Attachment**: Attach images from clipboard to your transcription
- **AI Integration**: Send transcriptions to OpenClaw agent via Podman
- **Audio Feedback**: Beep sounds to indicate recording state

## Supported ASR Backends

| Backend | Models | Strengths |
|---------|--------|-----------|
| **HuggingFace** (default) | Qwen3-ASR-1.7B, etc. | Modern |
| **Whisper** | tiny → large-v3 | Proven, widely used |

## Prerequisites

- **Python 3.11+**
- **Podman Desktop** (for OpenClaw container): [Download](https://podman-desktop.io/)
- **FFmpeg** (required by Whisper backend): `winget install Gyan.FFmpeg`
- **CUDA Toolkit** (optional, for GPU acceleration): [Download](https://developer.nvidia.com/cuda-downloads)

## Quick Start

### 1. Set up Python Environment

```powershell
.\setup.ps1
```

Or manually:

```powershell
python -m venv .venv
.\.venv\Scripts\Activate.ps1
pip install -r ptt/requirements.txt

# For GPU support with CUDA 12.8:
pip install torch torchvision torchaudio --index-url https://download.pytorch.org/whl/cu128

# (Flash Attention 2 removed from this project configuration.)
```

### 2. Set up OpenClaw Container

```powershell
.\container-setup.ps1
```

### 3. Run the Application

```powershell
.\.venv\Scripts\Activate.ps1
python ptt.py

# Or with debug mode (confirm before sending):
python ptt.py -d
```

## Usage

| Hotkey | Action |
|--------|--------|
| `Ctrl+Alt+Space` | Toggle recording (start/stop) |
| `Ctrl+Alt+I` | Attach clipboard image (while recording) |
| `Ctrl+Alt+U` | Toggle ASR model load/unload |
| `Escape` or `Ctrl+C` | Exit the application |

### Workflow

1. Press `Ctrl+Alt+Space` to start recording (you'll hear a beep)
2. Speak your message - transcription streams in real-time!
3. Optionally press `Ctrl+Alt+I` to attach an image from clipboard
4. Press `Ctrl+Alt+Space` again to stop recording
5. The complete transcription is sent to OpenClaw

## Configuration

Edit `ptt.toml` to customize settings:

```toml
[hotkey]
toggle = "<ctrl>+<alt>+<space>"
attach_clipboard_image = "<ctrl>+<alt>+i"
unload_model = "<ctrl>+<alt>+u"

[audio]
tmp_dir = "%TEMP%"
rate = 16000
channels = 1

# =============================================================================
# ASR (Automatic Speech Recognition) Configuration
# =============================================================================
[asr]
# Backend: "whisper" (OpenAI) or "huggingface" (Qwen3-ASR, etc.)
backend = "huggingface"
save_transcription = false

# -----------------------------------------------------------------------------
# Whisper Backend Settings (used when backend = "whisper")
# -----------------------------------------------------------------------------
[asr.whisper]
model = "large"           # tiny, base, small, medium, large, large-v2, large-v3
language = "fr"
force_fp32 = false        # false = use FP16 on GPU (faster)
initial_prompt = ""       # Guide transcription style
suppress_fillers = false  # Suppress "uh", "um", etc.

# -----------------------------------------------------------------------------
# HuggingFace Backend Settings (used when backend = "huggingface")
# -----------------------------------------------------------------------------
[asr.huggingface]
model = "Qwen/Qwen3-ASR-1.7B"
language = "fr"
torch_dtype = "float16"   # float16, bfloat16, or float32
device_map_auto = true    # Automatic GPU placement

[streaming]
chunk_duration = 5.0      # Seconds per chunk
overlap_duration = 1.0    # Overlap to avoid cutting words

[reconciler]
# Algorithm for merging overlapping segments
# Options: "word_overlap" (fast), "fuzzy" (handles typos), "llm" (best accuracy)
algorithm = "word_overlap"

# Word overlap settings
min_overlap_words = 3
max_context_words = 15

# Fuzzy settings (requires: pip install rapidfuzz)
fuzzy_threshold = 0.8

# LLM settings (requires: pip install transformers)
llm_model = "HuggingFaceTB/SmolLM2-360M-Instruct"
llm_device = "cuda"

[beep]
start_stop = true
every_seconds = 30
frequency = 800
duration_ms = 200

[clipboard]
prefix = "whisper_ptt_clip_"
delete_after_send = true

[openclaw]
send = true
container_name = "openclaw-agent"
session_id = "agent:main:main"
single_line = false
max_chars = 8000
shared_dir = "./shared"
```

## ASR Backend Comparison

### HuggingFace (Default)

Best for modern models like Qwen3-ASR.

```toml
[asr]
backend = "huggingface"

[asr.huggingface]
model = "Qwen/Qwen3-ASR-1.7B"
```

### OpenAI Whisper

Proven and reliable, supports multiple model sizes.

```toml
[asr]
backend = "whisper"

[asr.whisper]
model = "large"
initial_prompt = "Clear transcription without filler words."
suppress_fillers = true
```

## Whisper Models

| Model | Size | Speed | Accuracy |
|-------|------|-------|----------|
| tiny | 39M | Fastest | Basic |
| base | 74M | Fast | Good |
| small | 244M | Medium | Better |
| medium | 769M | Slow | Great |
| large | 1550M | Slowest | Best |

## Reconciler Algorithms

The reconciler merges overlapping transcription segments from streaming audio:

| Algorithm | Speed | Accuracy | Dependencies | Use Case |
|-----------|-------|----------|--------------|----------|
| `none` | Fastest | N/A | None | No reconciliation (no streaming) |
| `word_overlap` | Fast | Good | None | Default, reliable |
| `fuzzy` | Fast | Better | `rapidfuzz` | Handles transcription errors |
| `llm` | Slower | Best | `transformers` | Complex speech, accents |

### Example: How Reconciliation Works

```
Chunk 1: "Hello my name is John and I"
Chunk 2 (with overlap): "John and I work at the company"
Reconciled: "Hello my name is John and I work at the company"
```

## Text-to-Speech Server (OpenAI-Compatible)

This project also includes a FastAPI server that exposes an OpenAI-style TTS endpoint
backed by Qwen3-TTS-12Hz-1.7B-Base for sample-based voice cloning.

### Install

```powershell
pip install -r tts_server/requirements.txt
```

### Configure

Edit `tts.toml`:

```toml
[server]
host = "127.0.0.1"
port = 8001

[model]
model = "Qwen/Qwen3-TTS-12Hz-1.7B-Base"
device_map = "cuda:0"
torch_dtype = "bfloat16"
language = "English"
voices_dir = "./tts_server/voices"

[output]
response_format = "wav"
```

### Run

```powershell
python -m tts_server
```

### Example Request

```bash
curl http://127.0.0.1:8001/v1/audio/speech ^
  -H "Content-Type: application/json" ^
  -d "{\"input\":\"Hello world\",\"voice_sample\":\"my_voice\"}" ^
  --output output.wav
```

Notes:
- `voice_sample` is required — it selects a voice sample folder for cloning.
- Supported `response_format`: `wav`, `flac`, `ogg`, `pcm`

### Voice Samples

Place samples under `voices_dir`:

```
tts_server/voices/my_voice/
├── audio.wav
└── text.txt
```

You can also list available speakers/languages and the sample layout:

```bash
curl http://127.0.0.1:8001/v1/audio/voices
```

## Project Structure

```
transcrption/
├── ptt/                      # Main package
│   ├── __init__.py
│   ├── __main__.py           # Entry point (python -m ptt)
│   ├── config.py             # Configuration loader
│   ├── recorder.py           # Audio recording with chunking
│   ├── transcriber.py        # ASR backends (Whisper, HuggingFace)
│   ├── openclaw.py           # OpenClaw integration
│   ├── hotkeys.py            # Hotkey handling
│   ├── reconcilers/          # Text reconciliation
│   │   ├── __init__.py       # Factory function
│   │   ├── base.py           # Abstract base class
│   │   ├── word_overlap.py   # Word-based overlap detection
│   │   ├── fuzzy.py          # Fuzzy string matching
│   │   └── llm.py            # LLM-based reconciliation
│   └── utils/
│       ├── __init__.py
│       ├── audio.py          # Audio utilities
│       └── logging.py        # Logging setup
│   └── requirements.txt      # PTT dependencies
├── ptt.py                    # Simple launcher
├── ptt.toml                  # Configuration
├── tts_server/               # OpenAI-compatible TTS server
│   ├── __main__.py           # Entry point (python -m tts_server)
│   ├── config.py             # TTS server configuration
│   ├── model.py              # Qwen3 TTS wrapper
│   └── server.py             # FastAPI app
│   └── requirements.txt      # TTS server dependencies
├── tts.toml                  # TTS server configuration
├── setup.ps1
├── container-setup.ps1
├── Containerfile
├── shared/                   # Shared with container
│   └── logs/
└── README.md
```

## Troubleshooting

### No GPU detected (using CPU)
Install PyTorch with CUDA 12.8 support:
```powershell
pip install torch torchvision torchaudio --index-url https://download.pytorch.org/whl/cu128
```

### Flash Attention 2 not supported
Flash Attention 2 is no longer part of this project configuration.

### "podman not found"
Install Podman Desktop from https://podman-desktop.io/

### FFmpeg errors
```powershell
winget install Gyan.FFmpeg
# Restart PowerShell after installation
```

### Hotkeys not working
- Make sure no other application is capturing the same hotkey
- Try running as administrator

### No audio recording
- Check Windows sound settings for your microphone
- Verify the microphone is set as default input device

### HuggingFace model download slow
Models are cached in `~/.cache/huggingface/`. First download may take time depending on model size.

## License

MIT
