#!/usr/bin/env bash
# Bash script to set up the OpenClaw Podman container (Linux)
# This script builds the container image and prepares it for OpenClaw installation

set -euo pipefail

CONTAINER_NAME="openclaw-agent"
IMAGE_NAME="openclaw-agent"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
SHARED_DIR="${PROJECT_DIR}/plugins"

SUDO=""
if [ "${EUID:-$(id -u)}" -ne 0 ]; then
  if command -v sudo >/dev/null 2>&1; then
    SUDO="sudo"
  else
    echo "ERROR: This script requires admin privileges, but sudo is not available." >&2
    exit 1
  fi
fi
PODMAN="${SUDO} podman"

if [ ! -d "${SHARED_DIR}" ]; then
  mkdir -p "${SHARED_DIR}"
  echo "[container] Created plugins folder: ${SHARED_DIR}"
fi

# Check if Podman is available
if ! command -v podman >/dev/null 2>&1; then
  echo "ERROR: Podman is not installed or not in PATH." >&2
  echo "Install Podman Desktop from: https://podman-desktop.io/"
  exit 1
fi

echo "[container] Podman version:"
${PODMAN} --version

# Check if container already exists
existing_container="$(${PODMAN} ps -a --filter "name=${CONTAINER_NAME}" --format "{{.Names}}" 2>/dev/null || true)"
if [ "${existing_container}" = "${CONTAINER_NAME}" ]; then
  echo "[container] Container '${CONTAINER_NAME}' already exists."

  # Check if it's running
  running_container="$(${PODMAN} ps --filter "name=${CONTAINER_NAME}" --format "{{.Names}}" 2>/dev/null || true)"
  if [ "${running_container}" = "${CONTAINER_NAME}" ]; then
    echo "[container] Container is already running."
  else
    echo "[container] Starting existing container..."
    ${PODMAN} start "${CONTAINER_NAME}"
    echo "[container] Container started."
  fi
else
  # Build the container image
  echo "[container] Building container image '${IMAGE_NAME}'..."
  ${PODMAN} build -t "${IMAGE_NAME}" -f "${PROJECT_DIR}/Containerfile" "${PROJECT_DIR}"

  # Run the container with port forwarding and shared folder
  echo "[container] Creating and starting container '${CONTAINER_NAME}'..."
  echo "[container] Mounting shared folder: ${SHARED_DIR} -> /app/plugins"
  ${PODMAN} run -d --name "${CONTAINER_NAME}" --network host -v "${SHARED_DIR}:/app/plugins:z" "${IMAGE_NAME}"

  echo "[container] Container started successfully."
fi

echo ""
echo "============================================================"
echo "[container] Installing OpenClaw inside container..."
echo "============================================================"
echo ""

# Run the OpenClaw installer interactively
${PODMAN} exec -it "${CONTAINER_NAME}" bash -c "/tmp/install-openclaw.sh"

echo ""
echo "============================================================"
echo "[container] OpenClaw container setup complete!"
echo "============================================================"
echo ""
echo "Container '${CONTAINER_NAME}' is running and ready."
echo ""
echo "To run openclaw commands:"
echo "  ${PODMAN} exec -it ${CONTAINER_NAME} bash              # Open a shell"
echo "  ${PODMAN} exec ${CONTAINER_NAME} openclaw --help       # Show OpenClaw help"
echo ""
echo "Other useful commands:"
echo "  ${PODMAN} logs ${CONTAINER_NAME}                       # View container logs"
echo "  ${PODMAN} stop ${CONTAINER_NAME}                       # Stop the container"
echo "  ${PODMAN} start ${CONTAINER_NAME}                      # Start the container"
echo "  ${PODMAN} rm -f ${CONTAINER_NAME}                      # Remove the container"
echo ""
