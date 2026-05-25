# T0K3N-MCP

> Token-saving MCP server for AI coding tools — reduces token consumption by up to 87%

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![Token Savings](https://img.shields.io/badge/Token%20Savings-87.3%25-brightgreen)](.docs/benchmark_token_savings.md)

---

## Language / 言語

| Language | Link |
|----------|------|
| English | [README.en.md](README.en.md) |
| 日本語 | [README.ja.md](README.ja.md) |
| 中文 | [README.zh.md](README.zh.md) |
| 한국어 | [README.ko.md](README.ko.md) |

---

## Quick Start

```bash
git clone https://github.com/tonrakun/t0k3n-mcp
cd t0k3n-mcp
cargo build --release
```

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

For full documentation, select your language above.
