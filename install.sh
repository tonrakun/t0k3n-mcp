#!/usr/bin/env bash
set -euo pipefail

REPO="tonrakun/T0K3N-MCP"
BIN_NAME="t0k3n-mcp"
INSTALL_DIR="${HOME}/.${BIN_NAME}"

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
BIN_PATH="${INSTALL_DIR}/${BIN_NAME}"

if [ -f "$BIN_PATH" ]; then
    # Update: download new binary first, swap only on success
    echo "Updating ${BIN_NAME}..."
    TMP_PATH="${BIN_PATH}.new"
    curl -fsSL "$URL" -o "$TMP_PATH"
    chmod +x "$TMP_PATH"
    rm -f "$BIN_PATH"
    mv "$TMP_PATH" "$BIN_PATH"
    echo "Updated: ${BIN_PATH}"
else
    # Install: create folder, download binary, write desktop config
    echo "Installing ${BIN_NAME}..."
    mkdir -p "$INSTALL_DIR"
    curl -fsSL "$URL" -o "$BIN_PATH"
    chmod +x "$BIN_PATH"
    echo "Installed: ${BIN_PATH}"

    DESKTOP="${HOME}/Desktop"
    [ -d "$DESKTOP" ] || DESKTOP="${HOME}"
    cat > "${DESKTOP}/.mcp.json" <<EOF
{
  "mcpServers": {
    "t0k3n": {
      "command": "${BIN_PATH}",
      "args": []
    }
  }
}
EOF
    echo "MCP config written: ${DESKTOP}/.mcp.json"

    if [[ ":${PATH}:" != *":${INSTALL_DIR}:"* ]]; then
        echo ""
        echo "Add to your shell profile (~/.bashrc, ~/.zshrc, etc.):"
        echo "  export PATH=\"${INSTALL_DIR}:\${PATH}\""
    fi
fi
