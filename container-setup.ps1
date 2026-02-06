# PowerShell script to set up the OpenClaw Podman container
# This script builds the container image and prepares it for OpenClaw installation

$ErrorActionPreference = "Stop"

$CONTAINER_NAME = "openclaw-agent"
$IMAGE_NAME = "openclaw-agent"
$SCRIPT_DIR = Split-Path -Parent $MyInvocation.MyCommand.Path
$SHARED_DIR = Join-Path $SCRIPT_DIR "shared"

# Ensure shared folder exists
if (-not (Test-Path $SHARED_DIR)) {
    New-Item -ItemType Directory -Force -Path $SHARED_DIR | Out-Null
    Write-Host "[container] Created shared folder: $SHARED_DIR" -ForegroundColor Yellow
}

# Check if Podman is available
try {
    $null = podman --version 2>$null
} catch {
    Write-Error "ERROR: Podman is not installed or not in PATH."
    Write-Host "Install Podman Desktop from: https://podman-desktop.io/" -ForegroundColor Cyan
    exit 1
}

Write-Host "[container] Podman version:" -ForegroundColor Cyan
podman --version

# Check if container already exists
$existingContainer = podman ps -a --filter "name=$CONTAINER_NAME" --format "{{.Names}}" 2>$null
if ($existingContainer -eq $CONTAINER_NAME) {
    Write-Host "[container] Container '$CONTAINER_NAME' already exists." -ForegroundColor Yellow
    
    # Check if it's running
    $runningContainer = podman ps --filter "name=$CONTAINER_NAME" --format "{{.Names}}" 2>$null
    if ($runningContainer -eq $CONTAINER_NAME) {
        Write-Host "[container] Container is already running." -ForegroundColor Green
    } else {
        Write-Host "[container] Starting existing container..." -ForegroundColor Yellow
        podman start $CONTAINER_NAME
        Write-Host "[container] Container started." -ForegroundColor Green
    }
} else {
    # Build the container image
    Write-Host "[container] Building container image '$IMAGE_NAME'..." -ForegroundColor Yellow
    podman build -t $IMAGE_NAME -f Containerfile .
    
    if ($LASTEXITCODE -ne 0) {
        Write-Error "ERROR: Failed to build container image."
        exit 1
    }
    Write-Host "[container] Image built successfully." -ForegroundColor Green
    
    # Run the container with port forwarding and shared folder
    Write-Host "[container] Creating and starting container '$CONTAINER_NAME'..." -ForegroundColor Yellow
    Write-Host "[container] Mounting shared folder: $SHARED_DIR -> /app/shared" -ForegroundColor Yellow
    podman run -d --name $CONTAINER_NAME -p 1455:1455 -v "${SHARED_DIR}:/app/shared:z" $IMAGE_NAME
    
    if ($LASTEXITCODE -ne 0) {
        Write-Error "ERROR: Failed to start container."
        exit 1
    }
    Write-Host "[container] Container started successfully." -ForegroundColor Green
}

Write-Host ""
Write-Host "=" * 60 -ForegroundColor Cyan
Write-Host "[container] Installing OpenClaw inside container..." -ForegroundColor Cyan
Write-Host "=" * 60 -ForegroundColor Cyan
Write-Host ""

# Run the OpenClaw installer interactively
podman exec -it $CONTAINER_NAME bash -c "/tmp/install-openclaw.sh"

Write-Host ""
Write-Host "=" * 60 -ForegroundColor Green
Write-Host "[container] OpenClaw container setup complete!" -ForegroundColor Green
Write-Host "=" * 60 -ForegroundColor Green
Write-Host ""
Write-Host "Container '$CONTAINER_NAME' is running and ready." -ForegroundColor Cyan
Write-Host ""
Write-Host "To run openclaw commands:" -ForegroundColor Cyan
Write-Host "  podman exec -it $CONTAINER_NAME bash              # Open a shell" -ForegroundColor White
Write-Host "  podman exec $CONTAINER_NAME openclaw --help       # Show OpenClaw help" -ForegroundColor White
Write-Host ""
Write-Host "Other useful commands:" -ForegroundColor Cyan
Write-Host "  podman logs $CONTAINER_NAME                       # View container logs" -ForegroundColor White
Write-Host "  podman stop $CONTAINER_NAME                       # Stop the container" -ForegroundColor White
Write-Host "  podman start $CONTAINER_NAME                      # Start the container" -ForegroundColor White
Write-Host "  podman rm -f $CONTAINER_NAME                      # Remove the container" -ForegroundColor White
Write-Host ""
