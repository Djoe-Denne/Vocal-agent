#!/usr/bin/env bash
# Bash setup script for Push-to-Talk Speech-to-Text (Linux)
# Run this script to set up the Python virtual environment and install dependencies

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
VENV_DIR="${PROJECT_DIR}/.venv"
PYTHON_BIN="${PYTHON_BIN:-python3}"

# Check if Python is available
if ! command -v "${PYTHON_BIN}" >/dev/null 2>&1; then
  echo "ERROR: '${PYTHON_BIN}' not found. Install Python or set PYTHON_BIN environment variable." >&2
  exit 1
fi

echo "[setup] Using Python: ${PYTHON_BIN}"
"${PYTHON_BIN}" --version

# Create virtual environment if it doesn't exist (or if it was created on Windows)
activate_script="${VENV_DIR}/bin/activate"
if [ ! -d "${VENV_DIR}" ]; then
  echo "[setup] Creating virtual environment in ${VENV_DIR} ..."
  "${PYTHON_BIN}" -m venv "${VENV_DIR}"
elif [ ! -f "${activate_script}" ]; then
  echo "[setup] Existing virtual environment is not Linux-compatible. Recreating in ${VENV_DIR} ..."
  "${PYTHON_BIN}" -m venv "${VENV_DIR}"
else
  echo "[setup] Virtual environment already exists in ${VENV_DIR}"
fi

# Activate the virtual environment
if [ -f "${activate_script}" ]; then
  echo "[setup] Activating virtual environment..."
  # shellcheck disable=SC1090
  source "${activate_script}"
else
  echo "ERROR: Could not find activation script at ${activate_script}" >&2
  exit 1
fi

# Upgrade pip, setuptools, and wheel
echo "[setup] Upgrading pip, setuptools, and wheel..."
python -m pip install --upgrade pip setuptools wheel

echo ""
echo "============================================================"
echo "[setup] Python environment setup complete!"
echo "============================================================"
echo ""
echo "To activate the environment manually, run:"
echo "  source ./.venv/bin/activate"
echo ""
echo "Don't forget to set up the OpenClaw container:"
echo "  ./linux/container-setup.sh"
echo ""
