$ErrorActionPreference = "Stop"
$env:RUN_ENV = "development"

$repoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path

# Stop stale instances so ports are freed before restart.
$serviceNames = @("audio-service", "asr-service", "alignment-service", "orchestration-service")
foreach ($name in $serviceNames) {
    Get-Process -Name $name -ErrorAction SilentlyContinue | Stop-Process -Force
}

Start-Process cargo -WorkingDirectory (Join-Path $repoRoot "audio-service") -ArgumentList "run -p audio-setup --bin audio-service"
Start-Process cargo -WorkingDirectory (Join-Path $repoRoot "asr-service") -ArgumentList "run -p asr-setup --bin asr-service --features whisper-cuda"
Start-Process cargo -WorkingDirectory (Join-Path $repoRoot "alignment-service") -ArgumentList "run -p alignment-setup --bin alignment-service --features wav2vec2-cuda"
Start-Process cargo -WorkingDirectory (Join-Path $repoRoot "orchestration-service") -ArgumentList "run -p orchestration-setup --bin orchestration-service"