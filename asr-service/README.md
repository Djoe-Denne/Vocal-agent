# ASR Service

Transcription-only gRPC service in Rust.

## gRPC API

- Service: `asr.v1.AsrService`
- RPC: `Transcribe(TranscribeAudioRequest) -> TranscribeAudioResponse`
- Protobuf contract: `asr-service/proto/asr.proto`

`Transcribe` accepts raw audio samples and returns:

- `session_id`
- `transcript`
- `text`

## Crate layout

```
asr-service
├── setup             (application bootstrap and dependency wiring)
├── application       (commands, DTOs, use case)
├── domain            (core entities and transcription port contract)
├── infra-asr-whisper (whisper transcription adapter)
├── grpc              (tonic server + generated client/service stubs)
├── proto             (protobuf service contract)
└── configuration     (config structs and TOML loading)
```

## Run

The service bootstrap is exposed through `asr-setup::build_and_run`.
Call it from your runtime entrypoint after loading `asr_configuration::AppConfig`
and `rustycog_config::ServerConfig`.

## Test

```powershell
cargo test -p asr-application
```
