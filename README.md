# T0K3N-MCP

> Token-saving MCP server for AI coding tools — reduces token consumption by up to 87%

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![Token Savings](https://img.shields.io/badge/Token%20Savings-87.3%25-brightgreen)](.docs/benchmark_token_savings.md)

---

**日本語ドキュメント**: [README.ja.md](README.ja.md)

---

## Installation

**macOS / Linux**

```bash
curl -fsSL https://raw.githubusercontent.com/tonrakun/T0K3N-MCP/main/install.sh | bash
```

**Windows (PowerShell)**

```powershell
irm https://raw.githubusercontent.com/tonrakun/T0K3N-MCP/main/install.ps1 | iex
```

Installs to `~/.local/bin/t0k3n-mcp` (Unix) or `%USERPROFILE%\.local\bin\t0k3n-mcp.exe` (Windows) and adds it to your PATH.

<details>
<summary>Build from source</summary>

```bash
git clone https://github.com/tonrakun/t0k3n-mcp
cd t0k3n-mcp
cargo build --release
```

</details>

## Quick Start

Add to `.mcp.json`:

```json
{
  "mcpServers": {
    "t0k3n": {
      "command": "/path/to/t0k3n-mcp",
      "args": ["--root", "/path/to/your/project"]
    }
  }
}
```

## CLI Options

| Flag | Description |
|------|-------------|
| `--root <path>` | Workspace root directory (required) |
| `--no-dashboard` | Disable the web dashboard |
| `--open-browser` | Open the dashboard in a browser on startup |
| `--dashboard-port <port>` | Dashboard port (default: 14123) |
| `--list-tools` | Print all registered tool names and exit |
| `--refresh-parsers` | Clear the tree-sitter parser cache on startup |
