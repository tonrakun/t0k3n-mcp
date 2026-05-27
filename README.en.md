# T0K3N-MCP

> **MCP server that reduces token consumption in AI coding tools by up to 87%**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![Token Savings](https://img.shields.io/badge/Token%20Savings-87.3%25-brightgreen)](.docs/benchmark_token_savings.md)

**English** | [日本語](README.ja.md) | [中文](README.zh.md) | [한국어](README.ko.md)

---

## Benchmarks: 75–87% token reduction

Measured using the official Anthropic token-count API on **two real-world projects**.

### Study 1: Rust project (T0K3N-MCP itself)

| File | Full | T0K3N-MCP | Reduction |
|------|------|-----------|-----------|
| `code.rs` (295 lines) | 3,642 | 345 | **90.5%** |
| `mod.rs` (422 lines) | 4,997 | 1,162 | **76.7%** |
| `README.md` | 2,492 | 296 | **88.1%** |
| `Cargo.toml` | 491 | 24 | **95.1%** |
| **Average** | 2,147 | 321 | **87.3%** |

### Study 2: Next.js project (vercel/commerce)

| File | Full | T0K3N-MCP | Reduction |
|------|------|-----------|-----------|
| `components/cart/modal.tsx` | 2,776 | 143 | **94.8%** |
| `app/product/[handle]/page.tsx` | 1,400 | 134 | **90.4%** |
| `lib/shopify/index.ts` | 4,073 | 1,299 | **68.1%** |
| `components/cart/cart-context.tsx` | 1,742 | 488 | **72.0%** |
| **Average (20 files)** | 957 | 198 | **75.5%** |

### Full-project simulation (5-task investigation)

| | Standard | T0K3N-MCP | Reduction |
|-|----------|-----------|-----------|
| Next.js investigation | 19,109 tokens | 2,668 tokens | **86.0%** |

> Full methodology and data: [`.docs/benchmark_token_savings.md`](.docs/benchmark_token_savings.md)

A 200,000-token context window effectively becomes **6–8× larger**.

---

## Why standard tools fall short

Claude Code and Cursor's standard Read File dumps the entire file into context:

```
read_file("server/mod.rs")  →  4,997 tokens consumed
                                ↑ 95% unrelated to the current question
```

T0K3N-MCP solves this with a **"structure first, content on demand"** design:

```
read_code_skeleton("server/mod.rs")  →  1,162 tokens (signatures only)
read_code_body(["function:54-67"])   →    150 tokens (target function only)
                                         ────────────────────────────────
Total                                      1,312 tokens  ← 74% reduction
```

---

## Installation

### Pre-built binaries (recommended)

Download the binary for your OS from GitHub Releases.

| OS | File |
|----|------|
| macOS (Apple Silicon) | `t0k3n-mcp-macos-aarch64` |
| macOS (Intel) | `t0k3n-mcp-macos-x86_64` |
| Linux x86_64 | `t0k3n-mcp-linux-x86_64` |
| Linux ARM64 | `t0k3n-mcp-linux-aarch64` |
| Windows x86_64 | `t0k3n-mcp-windows-x86_64.exe` |

### Build from source

```bash
git clone https://github.com/tonrakun/t0k3n-mcp
cd t0k3n-mcp
cargo build --release
# → ./target/release/t0k3n-mcp
```

No dependencies beyond Rust. No Node.js, npm, or Python required.

---

## Setup

### Claude Code (`.mcp.json`)

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

### Cursor / Cline / Windsurf

Add the same configuration to each client's MCP settings file.

### Options

```
--root <path>          Workspace root (required)
```

---

## Usage

### Code files (Rust / Python / JS / TS / Go)

```
1. read_code_skeleton("path/to/file.rs")
   → Returns a list of function / struct / impl signatures + IDs

2. read_code_body(["function:10-45", "impl:87-130"])
   → Returns the body of only the specified functions
```

### Markdown / documentation

```
1. read_markdown_toc("ARCHITECTURE.md")
   → Returns a heading list with anchors

2. read_markdown_section("ARCHITECTURE.md", ["#database-design"])
   → Returns only the specified section
```

### Web pages

```
1. fetch_webpage("https://docs.rs/tokio/latest/tokio/")
   → Converts HTML to Markdown and returns the TOC

2. read_webpage_section(url, ["#struct-JoinHandle"])
   → Returns the specified section from the cached Markdown
```

### PDF / DOCX

```
1. convert_document("report.pdf")
   → Converts to Markdown and returns the TOC + tmp_path

2. read_markdown_section(tmp_path, ["#chapter-3"])
   → Returns only the specified section
```

### Token budget management

```
1. check_budget(budget=8000, candidates=["a.rs", "b.rs", "c.md"])
   → strategy: "full" | "skeleton_only" | "toc_only" | "skip"

2. Choose tools based on the recommended strategy
```

---

## Tool reference (31 tools)

### File reading

| Tool | Description |
|------|-------------|
| `read_directory_tree` | Directory tree with `.gitignore` filtering |
| `read_markdown_toc` | Markdown heading list (TOC) |
| `read_markdown_section` | Fetch section body by anchor |
| `read_code_skeleton` | Return function / class signatures only |
| `read_code_body` | Fetch function body by skeleton ID |
| `read_symbol_usages` | Find all usages of a symbol across the workspace |
| `read_code_deps` | import / imported_by dependency graph |
| `read_file_outline` | Unified outline with auto file-type detection |
| `read_git_diff` | Compressed git diff |
| `read_git_log` | Structured commit log (author, date, changed files) |
| `read_git_blame_body` | Per-line blame for a function body (author + date) |
| `search_file` | Keyword / regex match with surrounding context |
| `semantic_search` | Find semantically relevant functions via natural language |
| `read_json_yaml_keys` | List key structure of JSON/YAML/TOML |
| `read_json_yaml_value` | Fetch value by dot-notation key path (JSON/YAML/TOML) |
| `read_openapi` | Parse OpenAPI/Swagger spec into compact endpoint list |
| `read_env_schema` | Extract env var definitions from .env.example / docker-compose.yml |

### Web & documents

| Tool | Description |
|------|-------------|
| `fetch_webpage` | HTML → Markdown conversion + compression → TOC |
| `read_webpage_section` | Fetch section from cached web page |
| `convert_document` | PDF / DOCX → Markdown conversion |

### Text & budget

| Tool | Description |
|------|-------------|
| `compress_text` | Remove Markdown noise and excess whitespace |
| `count_tokens` | Token / character / line count |
| `check_budget` | Return remaining budget and recommended read strategy |
| `summarize_conversation` | Summarize conversation history within a token budget |

### Memory / tasks / sessions

| Tool | Description |
|------|-------------|
| `memory_save/get/list/delete` | SQLite-backed persistent key-value store |
| `task_create/update/get/list/delete` | Task management (status, priority, tags) |
| `session_snapshot/restore/list` | Save and restore working state |

---

## Supported languages

Languages supported by `read_code_skeleton` / `read_code_body` / `read_code_deps`:

| Language | Extensions |
|----------|------------|
| Rust | `.rs` |
| Python | `.py` |
| JavaScript | `.js`, `.jsx` |
| TypeScript | `.ts`, `.tsx` |
| Go | `.go` |
| C++ | `.cpp`, `.cc`, `.cxx`, `.hpp` |
| Java | `.java` |
| Ruby | `.rb` |
| C# | `.cs` |
| PHP | `.php` |

Parsers are statically bundled into the binary as Cargo crates at build time — no runtime download required. New language support is shipped in new releases. Request via [GitHub Issues](https://github.com/tonrakun/t0k3n-mcp/issues).

---

## Security

- All path resolution outside `--root` is blocked (path traversal protection)
- Symlink escapes outside root are blocked
- Only web tools (`fetch_webpage`) target URLs outside root (by design)

---

## Data storage

```
<root>/.t0k3n/
  t0k3n.db        ← SQLite (memory, tasks, sessions)
```

Recommended `.gitignore` entry:

```gitignore
.t0k3n/
```

---

## License

[MIT](LICENSE) © 2025 Tonrakun
