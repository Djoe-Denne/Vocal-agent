# Adapted Architecture — Re-dubbing Pipeline

## How it maps to the existing codebase

### Existing services (unchanged)
```
audio-service      → audio resampling/clamping (gRPC)
asr-service        → Whisper transcription (gRPC)
alignment-service  → wav2vec2 forced alignment (gRPC, candle)
orchestration-service → pipeline: audio_transform → asr_transcribe → alignment_enrich
```

### New services/crates

```
vocal-features/              ← Brick 2: pure DSP library crate (no service, no ML)
tts-service/                 ← Brick 3: Piper TTS + prosody application (gRPC service)
voice-conversion-service/    ← Brick 4: RVC wrapper (Python web service, POC only)
```

Plus a new orchestration pipeline definition that chains everything for re-dubbing.

---

## Brick 2 — `vocal-features` (library crate)

**Not a service.** Pure computation, no gRPC, no async. Used as a dependency by `tts-service` and potentially by `alignment-service` later.

Lives at workspace root alongside the services:

```
vocal-features/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── yin.rs              # YIN F0 estimator
    ├── energy.rs           # RMS energy
    ├── extractor.rs        # Per-word feature extraction
    └── util.rs             # ms_to_samples, hann_window
```

**Cargo.toml:**
```toml
[package]
name = "vocal-features"
version.workspace = true
edition.workspace = true

[dependencies]
thiserror = { workspace = true }
serde = { workspace = true }

[dev-dependencies]
approx = "0.5"
```

No dependency on `alignment-domain` or `rustycog-core`. Defines its own lightweight types:

```rust
// vocal-features/src/lib.rs
pub struct WordBoundary {
    pub text: String,
    pub start_ms: u64,   // u64 to match alignment-domain::WordTiming
    pub end_ms: u64,
}

pub struct ProsodyFeatures {
    pub f0_mean_hz: Option<f32>,
    pub f0_std_hz: Option<f32>,
    pub energy_rms: f32,
    pub voicing_ratio: f32,
}
```

Uses `u64` for timestamps (matching `alignment-domain::WordTiming::start_ms`/`end_ms`), not `u32` as in the earlier prompts.

**Conversion from alignment domain types** happens at the call site:
```rust
// In tts-service infra, not in vocal-features itself
let boundaries: Vec<WordBoundary> = aligned_words
    .iter()
    .map(|w| WordBoundary { text: w.word.clone(), start_ms: w.start_ms, end_ms: w.end_ms })
    .collect();
```

---

## Brick 3 — `tts-service` (gRPC service)

Follows the exact same structure as `alignment-service`:

```
tts-service/
├── domain/
│   ├── src/
│   │   ├── entity.rs       # TimedWord, TimedTranscript, SynthesisOutput, TtsRequest
│   │   ├── port.rs         # TtsSynthesisPort, ProsodyApplicationPort
│   │   └── lib.rs
│   └── Cargo.toml
├── application/
│   ├── src/
│   │   ├── command/
│   │   │   ├── synthesize_audio.rs
│   │   │   ├── factory.rs
│   │   │   └── mod.rs
│   │   ├── dto/
│   │   │   ├── synthesize.rs    # SynthesizeRequest, SynthesizeResponse
│   │   │   └── mod.rs
│   │   ├── usecase/
│   │   │   ├── synthesize.rs    # SynthesizeUseCase trait + impl
│   │   │   └── mod.rs
│   │   ├── error.rs
│   │   └── lib.rs
│   └── Cargo.toml
├── infra-tts-piper/
│   ├── src/
│   │   ├── lib.rs           # PiperTtsAdapter implements TtsSynthesisPort
│   │   ├── phonemizer.rs    # espeak-ng subprocess wrapper
│   │   └── config.rs        # PiperAdapterConfig, phoneme_id_map parsing
│   └── Cargo.toml
├── infra-prosody/
│   ├── src/
│   │   ├── lib.rs           # ProsodyApplicator implements ProsodyApplicationPort
│   │   ├── phase_vocoder.rs
│   │   └── gain.rs
│   └── Cargo.toml
├── grpc/
│   ├── src/lib.rs
│   ├── build.rs
│   └── Cargo.toml
├── proto/
│   └── tts.proto
├── configuration/
│   ├── src/lib.rs
│   └── Cargo.toml
├── setup/
│   ├── src/
│   │   ├── app.rs
│   │   ├── bin/tts-service.rs
│   │   └── lib.rs
│   └── Cargo.toml
└── config/
    ├── default.toml
    ├── development.toml
    ├── production.toml
    └── test.toml
```

### domain/src/entity.rs

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimedWord {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimedTranscript {
    pub total_duration_ms: u64,
    pub words: Vec<TimedWord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordProsody {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub f0_mean_hz: Option<f32>,
    pub f0_std_hz: Option<f32>,
    pub energy_rms: f32,
    pub voicing_ratio: f32,
}

#[derive(Debug, Clone)]
pub struct TtsRequest {
    pub transcript: TimedTranscript,
    pub prosody: Option<Vec<WordProsody>>,
    pub sample_rate_hz: u32,
}

#[derive(Debug, Clone)]
pub struct TtsOutput {
    pub samples: Vec<f32>,
    pub sample_rate_hz: u32,
    pub word_timings: Vec<SynthesizedWordTiming>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesizedWordTiming {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub fit_strategy: FitStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FitStrategy {
    Natural,
    Padded,
    Truncated,
    Stretched,
}
```

### domain/src/port.rs

```rust
use async_trait::async_trait;
use crate::{DomainError, TtsOutput, TtsRequest};

#[async_trait]
pub trait TtsSynthesisPort: Send + Sync {
    async fn synthesize(&self, request: TtsRequest) -> Result<TtsOutput, DomainError>;
}
```

Note: the port is async to match the codebase convention even though Piper inference is synchronous — the adapter wraps it with `tokio::task::spawn_blocking`.

### infra-tts-piper

**Key decision: `ort` (not candle) for Piper.**

The alignment service uses candle because wav2vec2 was implemented from scratch in candle. Piper ships as ONNX — reimplementing VITS in candle would be a multi-week effort with no benefit. Use `ort` here.

```toml
# infra-tts-piper/Cargo.toml
[dependencies]
tts-domain = { path = "../domain" }
vocal-features = { path = "../../vocal-features" }  # for F0/RMS on TTS output
async-trait = { workspace = true }
ort = "2"
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
```

The adapter:
1. Loads Piper ONNX model + JSON config at construction
2. Phonemizes via espeak-ng CLI subprocess
3. Maps IPA → phoneme IDs (with blank token interleaving)
4. Runs ONNX inference with duration fitting (length_scale adjustment)
5. Assembles output buffer with silence gaps
6. If prosody is provided: uses `vocal-features` to estimate source F0, then applies phase vocoder pitch shift + gain from `infra-prosody`

### infra-prosody

```toml
# infra-prosody/Cargo.toml
[dependencies]
tts-domain = { path = "../domain" }
vocal-features = { path = "../../vocal-features" }
rustfft = "6"
tracing = { workspace = true }
```

Pure DSP: phase vocoder + gain adjustment. No async, no ports — called directly by the Piper adapter after synthesis.

### proto/tts.proto

```protobuf
syntax = "proto3";
package tts.v1;

service TtsService {
    rpc SynthesizeAudio(SynthesizeAudioRequest) returns (SynthesizeAudioResponse);
}

message SynthesizeAudioRequest {
    TimedTranscript transcript = 1;
    repeated WordProsody prosody = 2;    // optional
    optional uint32 sample_rate_hz = 3;
    optional string session_id = 4;
}

message SynthesizeAudioResponse {
    string session_id = 1;
    repeated float samples = 2;
    uint32 sample_rate_hz = 3;
    repeated SynthesizedWordTiming word_timings = 4;
}

message TimedTranscript {
    uint64 total_duration_ms = 1;
    repeated TimedWord words = 2;
}

message TimedWord {
    string text = 1;
    uint64 start_ms = 2;
    uint64 end_ms = 3;
}

message WordProsody {
    string text = 1;
    uint64 start_ms = 2;
    uint64 end_ms = 3;
    optional float f0_mean_hz = 4;
    optional float f0_std_hz = 5;
    float energy_rms = 6;
    float voicing_ratio = 7;
}

message SynthesizedWordTiming {
    string text = 1;
    uint64 start_ms = 2;
    uint64 end_ms = 3;
    string fit_strategy = 4;
}
```

### config/default.toml

```toml
[server]
host = "127.0.0.1"
port = 8084
tls_enabled = false

[logging]
level = "info"

[tts]
sample_rate_hz = 22050
model_path = "../models/piper-voices/fr/fr_FR/siwis/medium/fr_FR-siwis-medium.onnx"
config_path = "../models/piper-voices/fr/fr_FR/siwis/medium/fr_FR-siwis-medium.onnx.json"
noise_scale = 0.667
noise_w = 0.8
min_length_scale = 0.5
truncation_fade_ms = 10
```

### Orchestration integration

Add a new pipeline definition and a new infra adapter in orchestration-service:

```
orchestration-service/
├── infra-tts/               ← NEW: gRPC client stage calling tts-service
│   ├── src/lib.rs           # TtsSynthesizeStage implements PipelineStage
│   └── Cargo.toml
```

New pipeline definition in orchestration config:

```toml
[service.pipeline.definitions.redub]
pre = ["audio_transform"]
transcription = "asr_transcribe"
post = ["alignment_enrich", "vocal_feature_extract", "tts_synthesize"]
```

The `vocal_feature_extract` stage would be an in-process stage (not a gRPC call) since `vocal-features` is a library. It runs YIN + RMS on the original audio using the aligned word boundaries and stores the results in `context.extensions`.

The `tts_synthesize` stage calls the tts-service via gRPC, passing the sanitized timed transcript + prosody features from context.

---

## Workspace additions

In the root `Cargo.toml` workspace members, add:

```toml
members = [
    # ... existing ...
    "vocal-features",
    "tts-service/domain",
    "tts-service/application",
    "tts-service/infra-tts-piper",
    "tts-service/infra-prosody",
    "tts-service/grpc",
    "tts-service/configuration",
    "tts-service/setup",
    "orchestration-service/infra-tts",
]
```

Add to workspace dependencies:

```toml
[workspace.dependencies]
# ... existing ...
ort = "2"
rustfft = "6"
hound = "3"
```

---

## Key differences from earlier prompts

| Earlier prompt | Adapted to codebase |
|---|---|
| `prosody-types` shared crate | Inlined into `vocal-features` + `tts-domain`. No separate shared types crate — each domain owns its types, conversion at boundaries (like alignment-domain vs orchestration-domain) |
| `u32` timestamps | `u64` timestamps (matching `WordTiming::start_ms` across all existing domains) |
| `ort` for everything | `ort` for Piper only. Alignment stays candle. Feature extraction is pure DSP |
| CLI binary | gRPC service + CLI in setup binary. Matches asr-service/alignment-service pattern |
| Standalone workspace | Integrated into existing workspace as new service |
| `anyhow` for errors | `DomainError` from `rustycog-core` in domain/infra, `ApplicationError` in application, `CommandError` mapping in command layer |
| Sync-only | Async port traits with `spawn_blocking` for CPU-bound inference (matching alignment-service pattern) |
| Direct espeak-ng | Same, but wrapped in adapter behind a trait for testability |
| Feature extraction in brick 3 | `vocal-features` as a workspace crate, depended on by both `infra-tts-piper` and a new orchestration stage |

---

## Build & run order

1. `vocal-features` — `cargo test -p vocal-features` (pure DSP, no deps)
2. `tts-service` — `cargo check -p tts-setup` then test with mock
3. `orchestration-service` with new pipeline — integration test with all services running

## What stays as a web service (not Rust)

**Voice conversion (brick 4):** Python + FastAPI container wrapping RVC/Seed-VC. Called by orchestration as an HTTP stage, not gRPC. Add a `VoiceConversionStage` to orchestration that POSTs WAV to the Python service and gets WAV back. This is intentionally outside the Rust workspace — it graduates to a Rust crate later when we port HuBERT + RMVPE + net_g to ONNX sessions.