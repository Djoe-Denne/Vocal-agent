#!/usr/bin/env bash
set -euo pipefail

VENV_DIR=".venv"
PYTHON_BIN="${PYTHON_BIN:-python3}"

if ! command -v "$PYTHON_BIN" >/dev/null 2>&1; then
  echo "ERROR: '$PYTHON_BIN' not found. Set PYTHON_BIN or install python3." >&2
  exit 1
fi

if [ ! -d "$VENV_DIR" ]; then
  echo "[setup] Creating venv in $VENV_DIR ..."
  "$PYTHON_BIN" -m venv "$VENV_DIR"
else
  echo "[setup] venv already exists in $VENV_DIR"
fi

# shellcheck disable=SC1091
source "$VENV_DIR/bin/activate"

echo "[setup] Upgrading pip/setuptools/wheel ..."
python -m pip install -U pip setuptools wheel

echo "[setup] Installing dependencies from ptt/requirements.txt ..."
python -m pip install -U -r ptt/requirements.txt

echo
echo "[setup] Done."
echo "Run: source $VENV_DIR/bin/activate && python stt_ptt.py"
