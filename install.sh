#!/usr/bin/env bash
# Thin bootstrap: download the binary, put it on PATH, done.
# Everything else (updates, .mcp.json) is handled by the binary itself:
#   t0k3n upgrade / t0k3n setup
set -euo pipefail

REPO="tonrakun/t0k3n-mcp"
INSTALL_DIR="${HOME}/.t0k3n-mcp"
BIN_PATH="${INSTALL_DIR}/t0k3n"

case "$(uname -s)" in
  Linux*)  OS_NAME="linux" ;;
  Darwin*) OS_NAME="macos" ;;
  *) echo "Unsupported OS: $(uname -s)" >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64)        ARCH_NAME="x86_64" ;;
  aarch64|arm64) ARCH_NAME="aarch64" ;;
  *) echo "Unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

URL="https://github.com/${REPO}/releases/latest/download/t0k3n-${OS_NAME}-${ARCH_NAME}"

echo ""
echo "Installing t0k3n..."
echo "  ${URL}"
mkdir -p "$INSTALL_DIR"
rm -f "${INSTALL_DIR}/VERSION"

TMP_PATH="${BIN_PATH}.new"
curl -fL --progress-bar "$URL" -o "$TMP_PATH"
SIZE="$(wc -c < "$TMP_PATH" | tr -d '[:space:]')"
if [ "$SIZE" -lt 1048576 ]; then
    rm -f "$TMP_PATH"
    echo "Downloaded file is too small (${SIZE} bytes) - not a valid binary" >&2
    exit 1
fi
chmod +x "$TMP_PATH"
# rename(2) atomically replaces the binary even while a server is running
mv -f "$TMP_PATH" "$BIN_PATH"

# Keep the legacy name working for existing .mcp.json configs
if [ -e "${INSTALL_DIR}/t0k3n-mcp" ]; then
    ln -sf "$BIN_PATH" "${INSTALL_DIR}/t0k3n-mcp"
fi

echo ""
echo "Install complete: $("$BIN_PATH" version)"
echo "  ${BIN_PATH}"
if [[ ":${PATH}:" != *":${INSTALL_DIR}:"* ]]; then
    echo ""
    echo "Add to your shell profile (~/.bashrc, ~/.zshrc, etc.):"
    echo "  export PATH=\"${INSTALL_DIR}:\${PATH}\""
fi
echo ""
echo "Next steps:"
echo "  t0k3n setup    # write .mcp.json in your project directory"
echo "  t0k3n upgrade  # update to the latest release"
echo ""
