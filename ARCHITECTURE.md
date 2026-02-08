# Architecture

This document describes the hexagonal (ports & adapters) architecture used across
the Rust crates. The design mirrors the Python `ptt`, `tts_server`, and
`shared` modules they replace.

---

## Design principles

1. **Domain at the centre** — business rules and data models live in `domain/`
   and depend on *nothing* external.
2. **Ports are traits** — abstract contracts (`trait`) defined in the domain that
   infrastructure adapters implement.
3. **Adapters are plug-ins** — each `infra_*` module provides one concrete
   implementation of a domain port. Swapping backends (e.g. a different ASR
   engine) means adding a new `infra_*` module without touching the domain.
4. **Application layer orchestrates** — services, configuration, and factories
   live in `application/`. They depend on domain traits, never on concrete
   adapters.
5. **`main.rs` is the composition root** — the binary wires adapters to services
   and runs the program. It is the only place that knows about concrete types.

---

## Project structure

`asr` and `tts` are **independent Cargo projects** — each has its own
`Cargo.toml`, `Cargo.lock`, and dependency tree. They cannot be in a single
workspace because `aha` requires candle 0.9.1 while `qwen3-tts` requires
candle 0.9.2, and these are mutually incompatible.

`shared_rs` is a library crate referenced by both via `path = "../shared_rs"`.

```
├── shared_rs/                 ← shared crate (pub name: "shared")
│   └── src/
│       ├── domain/
│       │   ├── pipeline.rs        Stage trait, PipelineContext, MediaType
│       │   └── pipeline_runner.rs Pipeline (ordered stage runner)
│       └── application/
│           └── pipeline_factory.rs StageRegistry, builder-based pipeline construction
│
├── asr/                       ← ASR crate (independent Cargo project)
│   ├── Cargo.toml                 candle 0.9.1, aha
│   ├── Cargo.lock
│   └── src/
│       ├── domain/
│       │   ├── value_objects.rs    ModelRef, Language, SampleRate
│       │   ├── models.rs          TranscriptionRequest, TranscriptionResult, ResolvedModel, TranscriptionOptions
│       │   ├── pipeline.rs        PreProcessorContext, PostProcessorContext
│       │   └── ports.rs           AsrEnginePort, ModelProviderPort, PreProcessor, PostProcessor
│       ├── application/
│       │   ├── config.rs          AsrConfig, ConfigService, TOML loader, override merging
│       │   ├── use_cases.rs       TranscribeAudioUseCase (orchestrator)
│       │   ├── model_resolver.rs  ModelResolver (routes Local, future HF)
│       │   └── pipeline_registry.rs PipelineRegistry (builds pre/post chains)
│       ├── infra_aha/
│       │   └── transcriber.rs     AhaTranscriber — AsrEnginePort adapter
│       ├── infra_local/
│       │   └── provider.rs        LocalModelProvider (ModelProviderPort)
│       ├── infra_cli/
│       │   └── cli.rs             Args — clap-based CLI adapter (subcommands: transcribe, serve)
│       ├── infra_web/
│       │   └── api.rs             Axum REST endpoints (POST /transcribe, GET /health)
│       ├── lib.rs                 Feature-gated module exports
│       └── main.rs                Composition root (CLI + web server)
│
└── tts/                       ← TTS crate (independent Cargo project)
    ├── Cargo.toml                 candle 0.9.2 (via qwen3-tts)
    ├── Cargo.lock
    └── src/
        ├── domain/
        │   ├── value_objects.rs    ModelRef, ModelId, VoiceId, Language, AudioFormat, SampleRate
        │   ├── models.rs          SynthesisRequest, SynthesisResult, ResolvedModel, SynthesisOptions
        │   ├── pipeline.rs        PreProcessorContext, PostProcessorContext
        │   └── ports.rs           TtsEnginePort, ModelProviderPort, PreProcessor, PostProcessor
        ├── application/
        │   ├── config.rs          TtsConfig, ConfigService, TOML loader, override merging
        │   ├── use_cases.rs       SynthesizeSpeechUseCase (orchestrator)
        │   ├── model_resolver.rs  ModelResolver (routes HF vs Local)
        │   └── pipeline_registry.rs PipelineRegistry (builds pre/post chains)
        ├── infra_qwen3/
        │   ├── engine.rs          Qwen3TtsEngine — TtsEnginePort, dispatch + model loading
        │   ├── engine_speaker.rs  Preset speaker synthesis (CustomVoice models)
        │   ├── engine_clone.rs    Voice clone synthesis from voice profiles (Base models)
        │   └── mapping.rs         Domain ↔ qwen3-tts type mapping (shared)
        ├── infra_hf/
        │   └── provider.rs        HuggingFaceModelProvider (ModelProviderPort)
        ├── infra_local/
        │   └── provider.rs        LocalModelProvider (ModelProviderPort)
        ├── infra_cli/
        │   └── cli.rs             CliArgs — clap-based CLI adapter
        ├── infra_web/
        │   └── api.rs             Axum REST endpoints (stub)
        ├── lib.rs
        └── main.rs                CLI composition root
```

---

## Layer-by-layer

### 1. Domain (`domain/`)

Pure Rust types and traits with **zero external dependencies** (except `anyhow`
for ergonomic errors).

| Crate | File | Contents |
|---|---|---|
| **shared** | `domain/pipeline.rs` | `Stage` trait, `PipelineContext`, `StageResult`, `MediaType` enum |
| **shared** | `domain/pipeline_runner.rs` | `Pipeline` — runs an ordered `Vec<Box<dyn Stage>>` with timing |
| **asr** | `domain/value_objects.rs` | `ModelRef`, `Language`, `SampleRate` |
| **asr** | `domain/models.rs` | `TranscriptionRequest`, `TranscriptionResult`, `ResolvedModel`, `TranscriptionOptions`, `TranscriptionTiming` |
| **asr** | `domain/pipeline.rs` | `PreProcessorContext`, `PostProcessorContext` |
| **asr** | `domain/ports.rs` | `AsrEnginePort`, `ModelProviderPort`, `PreProcessor`, `PostProcessor` traits |
| **tts** | `domain/value_objects.rs` | `ModelRef`, `ModelId`, `VoiceId`, `Language`, `AudioFormat`, `SampleRate` |
| **tts** | `domain/models.rs` | `SynthesisRequest`, `SynthesisResult`, `ResolvedModel`, `SynthesisOptions` |
| **tts** | `domain/pipeline.rs` | `PreProcessorContext`, `PostProcessorContext` |
| **tts** | `domain/ports.rs` | `TtsEnginePort`, `ModelProviderPort`, `PreProcessor`, `PostProcessor` traits |

#### Port traits

```text
  ┌──────────────────────────────────────────────────────────────────┐
  │                          Domain                                  │
  │                                                                  │
  │  trait AsrEnginePort            trait TtsEnginePort               │
  │    └ transcribe(model, request)   └ synthesize(model, request)   │
  │                                                                  │
  │  trait ModelProviderPort        trait ModelProviderPort           │
  │    └ prepare(model_ref)           └ prepare(model_ref)           │
  │                                                                  │
  │  trait PreProcessor             trait PreProcessor                │
  │    ├ name()                       ├ name()                       │
  │    └ process(PreProcessorCtx)     └ process(PreProcessorCtx)     │
  │                                                                  │
  │  trait PostProcessor            trait PostProcessor               │
  │    ├ name()                       ├ name()                       │
  │    └ process(PostProcessorCtx)    └ process(PostProcessorCtx)    │
  │                                                                  │
  │  trait Stage (shared)                                             │
  │    ├ name()                                                      │
  │    ├ input_type() / output_type()                                │
  │    ├ process(ctx) → ctx                                          │
  │    └ load() / unload()                                           │
  └──────────────────────────────────────────────────────────────────┘
```

> **Note:** Port traits require `Send` (movable across threads) but **not**
> `Sync`, because the upstream ML crates (`qwen3-tts`, `aha`) use `RefCell`
> internally. Thread-safe sharing can be achieved by wrapping adapters in
> `Mutex<T>` at the service level if needed.

### 2. Application (`application/`)

Orchestration logic that depends only on domain traits and models.

| Crate | File | Responsibility |
|---|---|---|
| **shared** | `application/pipeline_factory.rs` | `StageRegistry` — register builders, build `Pipeline` from config |
| **asr** | `application/config.rs` | `AsrConfig`, `ConfigService` — TOML config with override merging |
| **asr** | `application/use_cases.rs` | `TranscribeAudioUseCase` — orchestrates model resolution, pipeline, transcription |
| **asr** | `application/model_resolver.rs` | `ModelResolver` — routes `ModelRef` to local provider (future HF) |
| **asr** | `application/pipeline_registry.rs` | `PipelineRegistry` — builds pre/post processor chains from config |
| **tts** | `application/config.rs` | `TtsConfig`, `ConfigService` — TOML config with override merging |
| **tts** | `application/use_cases.rs` | `SynthesizeSpeechUseCase` — orchestrates model resolution, pipeline, synthesis |
| **tts** | `application/model_resolver.rs` | `ModelResolver` — routes `ModelRef` to HF or local provider |
| **tts** | `application/pipeline_registry.rs` | `PipelineRegistry` — builds pre/post processor chains from config |

The use cases never `use` a concrete adapter — they receive a trait object via
their constructor:

```rust
// asr/src/application/use_cases.rs
pub struct TranscribeAudioUseCase {
    config: AsrConfig,
    model_resolver: ModelResolver,
    engine: Box<dyn AsrEnginePort>,       // ← domain port, not AhaTranscriber
    pipeline_registry: PipelineRegistry,
}

// tts/src/application/use_cases.rs
pub struct SynthesizeSpeechUseCase {
    config: TtsConfig,
    model_resolver: ModelResolver,
    engine: Box<dyn TtsEnginePort>,       // ← domain port, not Qwen3TtsEngine
    pipeline_registry: PipelineRegistry,
}
```

### 3. Infrastructure (`infra_*/`)

Concrete implementations that bridge domain ports to external libraries.

| Crate | Module | Adapter | Implements | Backend |
|---|---|---|---|---|
| **asr** | `infra_aha/` | `AhaTranscriber` | `AsrEnginePort` | [`aha`](https://github.com/jhqxxx/aha) — candle 0.9.1 Qwen3-ASR |
| **asr** | `infra_local/` | `LocalModelProvider` | `ModelProviderPort` | Local directory validation |
| **asr** | `infra_cli/` | `Args`, `TranscribeArgs`, `ServeArgs` | — | Clap subcommands (transcribe, serve) |
| **asr** | `infra_web/` | Axum router | — | REST API (`POST /transcribe`, `GET /health`) |
| **tts** | `infra_qwen3/` | `Qwen3TtsEngine` | `TtsEnginePort` | [`qwen3-tts`](https://github.com/TrevorS/qwen3-tts-rs) — candle 0.9.2 Qwen3-TTS |
| **tts** | `infra_hf/` | `HuggingFaceModelProvider` | `ModelProviderPort` | HuggingFace Hub download + caching |
| **tts** | `infra_local/` | `LocalModelProvider` | `ModelProviderPort` | Local directory validation |

Adding a new backend (e.g. a Whisper adapter, an OpenAI API adapter) means:

1. Create a new `infra_<name>/` module.
2. Implement the relevant domain trait.
3. Wire it in `main.rs` — no other code changes needed.

### 4. Composition root (`main.rs`)

The binary entry point is the **only place** that knows about both the concrete
adapter *and* the application use case. It:

1. Parses CLI args (`clap`) — determines mode (transcribe vs serve).
2. Loads TOML config and applies CLI / env overrides.
3. Constructs concrete adapters and providers.
4. Injects them into the use case as `Box<dyn Port>`.
5. Runs the workflow (one-shot transcription or web server).

```text
  main.rs (ASR)
    │
    ├─ parse CLI args    (infra_cli::Args → Command::Transcribe | Command::Serve)
    ├─ load config       (application::ConfigService)
    │
    ├─ create provider   ←── infra_local::LocalModelProvider
    ├─ create resolver   ←── application::ModelResolver(local)
    ├─ create engine     ←── infra_aha::AhaTranscriber
    ├─ create registry   ←── application::PipelineRegistry (empty)
    │
    ├─ create use case   ←── application::TranscribeAudioUseCase(
    │                            config, resolver, Box<dyn AsrEnginePort>, registry)
    │
    ├─[transcribe mode]  build request → use_case.execute(request)
    └─[serve mode]       wrap use case in Arc<Mutex> → start Axum server

  main.rs (TTS)
    │
    ├─ parse CLI args    (infra_cli::CliArgs)
    ├─ load config       (application::ConfigService)
    │
    ├─ create providers  ←── infra_hf::HuggingFaceModelProvider
    │                    ←── infra_local::LocalModelProvider
    ├─ create resolver   ←── application::ModelResolver(hf, local)
    ├─ create engine     ←── infra_qwen3::Qwen3TtsEngine
    │
    ├─ create use case   ←── application::SynthesizeSpeechUseCase(
    │                            config, resolver, Box<dyn TtsEnginePort>, registry)
    ├─ build request     (merge config defaults + CLI overrides)
    └─ use_case.execute(request)
```

---

## Shared pipeline engine

The `shared` crate provides a reusable **pipeline pattern** for chaining
processing stages (pre-processors, models, post-processors):

```text
  PipelineContext ──► Stage 1 ──► Stage 2 ──► … ──► Stage N ──► PipelineContext
                      (timed)     (timed)            (timed)
```

- **`Stage`** — trait with `process(ctx) → ctx`, optional `load()`/`unload()`.
- **`Pipeline`** — named sequence of `Box<dyn Stage>`, runs all stages in order
  and records per-stage timing in `ctx.stage_results`.
- **`StageRegistry`** — maps type names to builder functions; constructs
  pipelines from TOML configuration.

This mirrors the Python `shared.domain.pipeline` / `shared.application.pipeline_factory`.

---

## Dependency graph

```text
  ┌──────────┐         ┌──────────┐
  │   asr    │         │   tts    │       ← independent Cargo projects
  │candle 0.9.1        │candle 0.9.2      (separate Cargo.lock each)
  └────┬─────┘         └────┬─────┘
       │                    │
       ├── aha              ├── qwen3-tts
       │                    │
       ▼                    ▼
  ┌───────────────────────────────┐
  │           shared              │       ← library crate (path dep)
  └───────────────────────────────┘
```

Each crate specifies its own dependency versions directly in its `Cargo.toml`.
There is **no workspace** — `asr` and `tts` are compiled separately because:

- `aha` requires candle **0.9.1** (non-exhaustive `match DType` breaks on 0.9.2+)
- `qwen3-tts` requires candle **0.9.2** (7-arg `sdpa()` API)

---

## Model loading

| Crate | Model source | Default |
|---|---|---|
| **asr** | Local directory | `./models/Qwen3-ASR-1.7b` |
| **tts** | HuggingFace Hub (auto-download) or local path | `Qwen/Qwen3-TTS-12Hz-1.7B-Base` |

The TTS `model_id` config field accepts either a HuggingFace model ID
(e.g. `Qwen/Qwen3-TTS-12Hz-1.7B-Base`) or a local directory path. When a HF
ID is used, the model files are downloaded from HuggingFace Hub and assembled
into a local staging directory (`engine.model_cache_dir`) that
`Qwen3TTS::from_pretrained()` can load directly.

### Voice profiles

Voice clone profiles are stored in `engine.voices_dir` (default `./voices/`):

```
voices/
  <voice_name>/
    reference.wav        ← required: reference audio sample
    transcript.txt       ← optional: transcript for ICL mode (better quality)
```

From the user's perspective, cloned voices work identically to preset speakers
(`--voice justamon` vs `--voice ryan`). The engine transparently resolves
the voice: preset names dispatch to `engine_speaker`, custom names look up a
profile in `voices/` and dispatch to `engine_clone`.

---

## Python ↔ Rust mapping

| Python module | Rust crate | Notes |
|---|---|---|
| `shared/domain/pipeline.py` | `shared_rs/src/domain/pipeline.rs` | `Stage`, `PipelineContext`, `MediaType` |
| `shared/application/pipeline_factory.py` | `shared_rs/src/application/pipeline_factory.rs` | `StageRegistry` |
| `ptt/domain/ports.py` | `asr/src/domain/ports.rs` | `AsrEnginePort`, `ModelProviderPort`, `PreProcessor`, `PostProcessor` |
| `ptt/domain/models.py` | `asr/src/domain/models.rs` | `TranscriptionRequest`, `TranscriptionResult`, `ResolvedModel` |
| `ptt/application/ptt_app.py` | `asr/src/application/use_cases.rs` | `TranscribeAudioUseCase` |
| `ptt/infra_huggingface/transcriber.py` | `asr/src/infra_aha/transcriber.rs` | `AhaTranscriber` (different backend) |
| `tts_server/domain/ports.py` | `tts/src/domain/ports.rs` | `TtsEnginePort`, `ModelProviderPort`, `PreProcessor`, `PostProcessor` |
| `tts_server/domain/models.py` | `tts/src/domain/models.rs` | `SynthesisRequest`, `SynthesisResult`, `ResolvedModel` |
| `tts_server/application/tts_service.py` | `tts/src/application/use_cases.rs` | `SynthesizeSpeechUseCase` |
| `tts_server/infra_pytorch/model.py` | `tts/src/infra_qwen3/engine.rs` | `Qwen3TtsEngine` (candle backend) |

---

## Adding a new adapter

Example: adding an OpenAI Whisper API adapter for ASR.

```
asr/src/
  infra_openai/
    mod.rs
    transcriber.rs   ← implements AsrEnginePort trait
```

1. **Create** `asr/src/infra_openai/mod.rs` and `transcriber.rs`.
2. **Implement** `AsrEnginePort` for your new struct.
3. **Register** the module in `asr/src/lib.rs`: `pub mod infra_openai;`.
4. **Wire** in `asr/src/main.rs`:
   ```rust
   let engine = OpenAiTranscriber::new(api_key);
   // inject into use case as Box<dyn AsrEnginePort>
   let use_case = TranscribeAudioUseCase::new(
       config, model_resolver, Box::new(engine), pipeline_registry,
   );
   ```
5. No changes to `domain/`, `application/`, or `shared`.

---

## Feature gates

Both `asr` and `tts` use Cargo feature gates for optional infrastructure:

| Feature | ASR | TTS | Dependencies |
|---|---|---|---|
| `cli` | CLI subcommands (transcribe) | CLI entry point | `clap` |
| `web` | Axum web server (serve) | Axum endpoints (stub) | `axum`, `tokio` |
| `cuda` | GPU acceleration | GPU acceleration | backend-specific |

Default features: `cli`, `web`, `cuda` (ASR); `cli`, `hub`, `cuda` (TTS).
