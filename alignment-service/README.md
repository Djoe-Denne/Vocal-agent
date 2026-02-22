# Alignment Service

Forced-alignment gRPC service built in Rust.

## gRPC API

- Service: `alignment.v1.AlignmentService`
- RPC: `EnrichTranscript(EnrichTranscriptRequest) -> EnrichTranscriptResponse`
- Protobuf contract: `alignment-service/proto/alignment.proto`

`EnrichTranscript` takes:
- audio samples (`samples`, `sample_rate_hz`)
- a transcript (`transcript`)

and returns:
- `session_id`
- `transcript`
- `aligned_words`
- `text`

## Architecture

```
alignment-service
├── setup            (application wiring)
├── application      (command/use-case layer)
├── configuration    (TOML config loading)
├── domain           (alignment entities + port trait)
├── grpc             (tonic server + generated client/service stubs)
├── proto            (protobuf service contract)
└── infra-alignment  (Wav2Vec2 forced aligner adapter)
```

## Build

```powershell
cargo check --workspace
```

## Run

```powershell
$env:RUN_ENV="development"
cargo run -p alignment-setup
```
