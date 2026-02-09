# PowerShell setup script for Push-to-Talk Speech-to-Text (Windows)
# Run this script to set up the Python virtual environment and install dependencies

$ErrorActionPreference = "Stop"

$SCRIPT_DIR = Split-Path -Parent $MyInvocation.MyCommand.Path
$PROJECT_DIR = Split-Path -Parent $SCRIPT_DIR
$VENV_DIR = Join-Path $PROJECT_DIR ".venv"
$PYTHON_BIN = if ($env:PYTHON_BIN) { $env:PYTHON_BIN } else { "python" }

# Check if Python is available
try {
    $null = & $PYTHON_BIN --version 2>$null
} catch {
    Write-Error "ERROR: '$PYTHON_BIN' not found. Install Python or set PYTHON_BIN environment variable."
    exit 1
}

Write-Host "[setup] Using Python: $PYTHON_BIN" -ForegroundColor Cyan
& $PYTHON_BIN --version

# Create virtual environment if it doesn't exist (or if it was created on Linux)
$activateScript = Join-Path $VENV_DIR "Scripts\Activate.ps1"
if (-not (Test-Path $VENV_DIR)) {
    Write-Host "[setup] Creating virtual environment in $VENV_DIR ..." -ForegroundColor Yellow
    & $PYTHON_BIN -m venv $VENV_DIR
} elseif (-not (Test-Path $activateScript)) {
    Write-Host "[setup] Existing virtual environment is not Windows-compatible. Recreating in $VENV_DIR ..." -ForegroundColor Yellow
    & $PYTHON_BIN -m venv $VENV_DIR
} else {
    Write-Host "[setup] Virtual environment already exists in $VENV_DIR" -ForegroundColor Green
}

# Activate the virtual environment
if (Test-Path $activateScript) {
    Write-Host "[setup] Activating virtual environment..." -ForegroundColor Yellow
    . $activateScript
} else {
    Write-Error "ERROR: Could not find activation script at $activateScript"
    exit 1
}

# Upgrade pip, setuptools, and wheel
Write-Host "[setup] Upgrading pip, setuptools, and wheel..." -ForegroundColor Yellow
python -m pip install --upgrade pip setuptools wheel


Write-Host ""
Write-Host "=" * 60 -ForegroundColor Green
Write-Host "[setup] Python environment setup complete!" -ForegroundColor Green
Write-Host "=" * 60 -ForegroundColor Green
Write-Host ""
Write-Host "To activate the environment manually, run:" -ForegroundColor Cyan
Write-Host "  .\.venv\Scripts\Activate.ps1" -ForegroundColor White
Write-Host ""
Write-Host "Don't forget to set up the OpenClaw container:" -ForegroundColor Cyan
Write-Host "  .\windows\container-setup.ps1" -ForegroundColor White
Write-Host ""
