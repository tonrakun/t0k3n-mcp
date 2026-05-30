#!/usr/bin/env bash
set -euo pipefail

REPO="tonrakun/T0K3N-MCP"
BIN_NAME="t0k3n-mcp"
INSTALL_DIR="${HOME}/.local/bin"

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux*)  OS_NAME="linux" ;;
  Darwin*) OS_NAME="macos" ;;
  *) echo "Unsupported OS: $OS" >&2; exit 1 ;;
esac

case "$ARCH" in
  x86_64)        ARCH_NAME="x86_64" ;;
  aarch64|arm64) ARCH_NAME="aarch64" ;;
  *) echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

ARTIFACT="${BIN_NAME}-${OS_NAME}-${ARCH_NAME}"
URL="https://github.com/${REPO}/releases/latest/download/${ARTIFACT}"

echo "Downloading ${ARTIFACT}..."
mkdir -p "$INSTALL_DIR"
curl -fsSL "$URL" -o "${INSTALL_DIR}/${BIN_NAME}"
chmod +x "${INSTALL_DIR}/${BIN_NAME}"

echo "Installed: ${INSTALL_DIR}/${BIN_NAME}"

if [[ ":${PATH}:" != *":${INSTALL_DIR}:"* ]]; then
  echo ""
  echo "Add to your shell profile (~/.bashrc, ~/.zshrc, etc.):"
  echo "  export PATH=\"\${HOME}/.local/bin:\${PATH}\""
fi
