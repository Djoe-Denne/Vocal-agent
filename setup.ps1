# PowerShell setup script for Push-to-Talk Speech-to-Text (Windows)
# Run this script to set up the Python virtual environment and install dependencies

$ErrorActionPreference = "Stop"

$VENV_DIR = ".venv"
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

# Create virtual environment if it doesn't exist
if (-not (Test-Path $VENV_DIR)) {
    Write-Host "[setup] Creating virtual environment in $VENV_DIR ..." -ForegroundColor Yellow
    & $PYTHON_BIN -m venv $VENV_DIR
} else {
    Write-Host "[setup] Virtual environment already exists in $VENV_DIR" -ForegroundColor Green
}

# Activate the virtual environment
$activateScript = Join-Path $VENV_DIR "Scripts\Activate.ps1"
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

# Install dependencies from ptt/requirements.txt
Write-Host "[setup] Installing dependencies from ptt/requirements.txt..." -ForegroundColor Yellow
python -m pip install --upgrade -r ptt/requirements.txt

Write-Host ""
Write-Host "=" * 60 -ForegroundColor Green
Write-Host "[setup] Python environment setup complete!" -ForegroundColor Green
Write-Host "=" * 60 -ForegroundColor Green
Write-Host ""
Write-Host "To activate the environment manually, run:" -ForegroundColor Cyan
Write-Host "  .\.venv\Scripts\Activate.ps1" -ForegroundColor White
Write-Host ""
Write-Host "To run the PTT script:" -ForegroundColor Cyan
Write-Host "  python ptt.py" -ForegroundColor White
Write-Host ""
Write-Host "Don't forget to set up the OpenClaw container:" -ForegroundColor Cyan
Write-Host "  .\container-setup.ps1" -ForegroundColor White
Write-Host ""

