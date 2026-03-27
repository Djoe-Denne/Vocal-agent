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
└── infra-alignment  (AlignmentPort adapter delegating to `wav2vec2-rs`)
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

Alignment defaults to the ONNX runtime backend. The Candle alignment path is removed.
Set model files to the shared ONNX package at:
`models/asr-wav2vec2-ctc-french-onnx/model.onnx`,
`models/asr-wav2vec2-ctc-french-onnx/config.json`,
and `models/asr-wav2vec2-ctc-french-onnx/vocab.json`.

If you need to export ONNX from Hugging Face/local weights, use:
`https://github.com/Djoe-Denne/wav2vec2-rs/blob/main/scripts/export_ctc_model_to_onnx.py`

To enable ONNX inference with BP/DP on WGPU:

```powershell
cargo run -p alignment-setup --features wav2vec2-onnx-wgpu-bp
```
