#!/usr/bin/env bash
set -euo pipefail

REPO="tonrakun/T0K3N-MCP"
BIN_NAME="t0k3n-mcp"
INSTALL_DIR="${HOME}/.${BIN_NAME}"
BIN_PATH="${INSTALL_DIR}/${BIN_NAME}"
VERSION_FILE="${INSTALL_DIR}/VERSION"
TOTAL_STEPS=4

if [ -t 1 ]; then
    C_CYAN=$'\033[36m'; C_GREEN=$'\033[32m'; C_RED=$'\033[31m'; C_GRAY=$'\033[90m'; C_RESET=$'\033[0m'
else
    C_CYAN=""; C_GREEN=""; C_RED=""; C_GRAY=""; C_RESET=""
fi

step() { printf '%s[%s/%s]%s %s\n' "$C_CYAN" "$1" "$TOTAL_STEPS" "$C_RESET" "$2"; }
ok()   { printf '      %sOK%s  %s\n' "$C_GREEN" "$C_RESET" "$1"; }
info() { printf '          %s%s%s\n' "$C_GRAY" "$1" "$C_RESET"; }
fail() { printf '      %sNG%s  %s\n' "$C_RED" "$C_RESET" "$1" >&2; exit 1; }

# ── Platform detection ────────────────────────────────────────────────────────
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux*)  OS_NAME="linux" ;;
  Darwin*) OS_NAME="macos" ;;
  *) fail "Unsupported OS: $OS" ;;
esac

case "$ARCH" in
  x86_64)        ARCH_NAME="x86_64" ;;
  aarch64|arm64) ARCH_NAME="aarch64" ;;
  *) fail "Unsupported architecture: $ARCH" ;;
esac

ARTIFACT="${BIN_NAME}-${OS_NAME}-${ARCH_NAME}"

IS_UPDATE=0
[ -f "$BIN_PATH" ] && IS_UPDATE=1

echo ""
if [ "$IS_UPDATE" = 1 ]; then
    echo "${BIN_NAME} installer - update"
else
    echo "${BIN_NAME} installer - fresh install"
fi
echo ""

# ── 1. Resolve latest release ────────────────────────────────────────────────
step 1 "Checking latest release..."
LATEST_VERSION=""
if API_JSON="$(curl -fsSL --max-time 15 -H 'User-Agent: t0k3n-mcp-installer' \
        "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null)"; then
    LATEST_VERSION="$(printf '%s' "$API_JSON" \
        | sed -n 's/.*"tag_name"[: ]*"v\{0,1\}\([^"]*\)".*/\1/p' | head -n1)"
fi
if [ -n "$LATEST_VERSION" ]; then
    ok "Latest release: v${LATEST_VERSION}"
else
    info "GitHub API unavailable — continuing without version check"
fi

# ── 2. Check installed version ───────────────────────────────────────────────
step 2 "Checking installed version..."
INSTALLED_VERSION=""
if [ "$IS_UPDATE" = 1 ]; then
    if [ -f "$VERSION_FILE" ]; then
        INSTALLED_VERSION="$(tr -d '[:space:]' < "$VERSION_FILE")"
    fi
    if [ -n "$INSTALLED_VERSION" ]; then
        ok "Installed: v${INSTALLED_VERSION}"
    else
        info "Installed version unknown (pre-2.5.0 binary)"
    fi
    if [ -n "$LATEST_VERSION" ] && [ "$INSTALLED_VERSION" = "$LATEST_VERSION" ]; then
        echo ""
        echo "${C_GREEN}Already up to date (v${INSTALLED_VERSION}). Nothing to do.${C_RESET}"
        exit 0
    fi
else
    ok "No existing install found"
    mkdir -p "$INSTALL_DIR"
fi

# ── 3. Download ──────────────────────────────────────────────────────────────
if [ -n "$LATEST_VERSION" ]; then
    URL="https://github.com/${REPO}/releases/download/v${LATEST_VERSION}/${ARTIFACT}"
    VERSION_LABEL="v${LATEST_VERSION}"
else
    URL="https://github.com/${REPO}/releases/latest/download/${ARTIFACT}"
    VERSION_LABEL="latest"
fi
step 3 "Downloading ${VERSION_LABEL}..."
info "$URL"
TMP_PATH="${BIN_PATH}.new"
if ! curl -fL --progress-bar "$URL" -o "$TMP_PATH"; then
    rm -f "$TMP_PATH"
    fail "Download failed"
fi
SIZE="$(wc -c < "$TMP_PATH" | tr -d '[:space:]')"
if [ "$SIZE" -lt 1048576 ]; then
    rm -f "$TMP_PATH"
    fail "Downloaded file is too small (${SIZE} bytes) — not a valid binary"
fi
ok "Downloaded $(awk "BEGIN { printf \"%.1f\", ${SIZE} / 1048576 }") MB"

# ── 4. Install / swap ────────────────────────────────────────────────────────
step 4 "Installing..."
chmod +x "$TMP_PATH"
# rename(2) atomically replaces the binary even while a server is running
mv -f "$TMP_PATH" "$BIN_PATH"

# Verify the new binary actually runs
NEW_VERSION=""
if OUT="$("$BIN_PATH" --version 2>/dev/null)"; then
    NEW_VERSION="$(printf '%s' "$OUT" | sed -n 's/.*\([0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\).*/\1/p' | head -n1)"
fi
if [ -n "$NEW_VERSION" ]; then
    printf '%s\n' "$NEW_VERSION" > "$VERSION_FILE"
    ok "Verified: ${BIN_NAME} v${NEW_VERSION}"
elif [ -n "$LATEST_VERSION" ]; then
    printf '%s\n' "$LATEST_VERSION" > "$VERSION_FILE"
    info "Binary installed (version probe unavailable on this release)"
else
    info "Binary installed"
fi

# First-install extras: MCP config + PATH hint
if [ "$IS_UPDATE" = 0 ]; then
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
    ok "MCP config written: ${DESKTOP}/.mcp.json"

    if [[ ":${PATH}:" != *":${INSTALL_DIR}:"* ]]; then
        echo ""
        echo "Add to your shell profile (~/.bashrc, ~/.zshrc, etc.):"
        echo "  export PATH=\"${INSTALL_DIR}:\${PATH}\""
    fi
fi

echo ""
if [ "$IS_UPDATE" = 1 ]; then
    FROM_LABEL="previous version"
    [ -n "$INSTALLED_VERSION" ] && FROM_LABEL="v${INSTALLED_VERSION}"
    TO_LABEL="latest"
    [ -n "$NEW_VERSION" ] && TO_LABEL="v${NEW_VERSION}"
    echo "${C_GREEN}Update complete: ${FROM_LABEL} -> ${TO_LABEL}${C_RESET}"
    echo "Restart Claude Code (or your MCP client) to load the new binary."
else
    echo "${C_GREEN}Install complete: ${BIN_PATH}${C_RESET}"
fi
echo ""
