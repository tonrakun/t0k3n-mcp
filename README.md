# T0K3N-MCP

> Token-saving MCP server for AI coding tools — reduces token consumption by up to 87%

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![Token Savings](https://img.shields.io/badge/Token%20Savings-87.3%25-brightgreen)](.docs/benchmark_token_savings.md)

---

**日本語ドキュメント**: [README.ja.md](README.ja.md)

---

## Measured: 75–87% Token Reduction

Benchmarked against **2 real projects** using Anthropic's official token-count API.

### Study 1: Rust Project (T0K3N-MCP itself)

| File | Full | T0K3N-MCP | Reduction |
|------|------|-----------|-----------|
| `code.rs` (295 lines) | 3,642 | 345 | **90.5%** |
| `mod.rs` (422 lines) | 4,997 | 1,162 | **76.7%** |
| `README.md` | 2,492 | 296 | **88.1%** |
| `Cargo.toml` | 491 | 24 | **95.1%** |
| **Average** | 2,147 | 321 | **87.3%** |

### Study 2: Next.js Project (vercel/commerce)

| File | Full | T0K3N-MCP | Reduction |
|------|------|-----------|-----------|
| `components/cart/modal.tsx` | 2,776 | 143 | **94.8%** |
| `app/product/[handle]/page.tsx` | 1,400 | 134 | **90.4%** |
| `lib/shopify/index.ts` | 4,073 | 1,299 | **68.1%** |
| `components/cart/cart-context.tsx` | 1,742 | 488 | **72.0%** |
| **Average (20 files)** | 957 | 198 | **75.5%** |

### Full-Project Simulation (5-Task Study)

| | Standard | T0K3N-MCP | Reduction |
|-|----------|-----------|-----------|
| Next.js investigation | 19,109 tokens | 2,668 tokens | **86.0%** |

> Full methodology and data: [`.docs/benchmark_token_savings.md`](.docs/benchmark_token_savings.md)

A 200,000-token context window effectively becomes **6–8× larger**.

---

## Why Standard Tools Fall Short

Claude Code and Cursor's built-in Read File dumps entire files into context.

```
read_file("server/mod.rs")  →  4,997 tokens consumed
                                ↑ 95% irrelevant to the current question
```

T0K3N-MCP solves this with **"structure first, fetch only what you need"**:

```
read_code_skeleton("server/mod.rs")  →  1,162 tokens (signatures only)
read_code_body(["function:54-67"])   →    150 tokens (target function only)
                                         ─────────────────────────────────
Total                                      1,312 tokens  ← 74% reduction
```

---

## Installation

**macOS / Linux**

```bash
curl -fsSL https://raw.githubusercontent.com/tonrakun/t0k3n-mcp/main/install.sh | bash
```

**Windows (PowerShell)**

```powershell
irm https://raw.githubusercontent.com/tonrakun/t0k3n-mcp/main/install.ps1 | iex
```

Installs to `~/.t0k3n-mcp/t0k3n` (Unix) or `%USERPROFILE%\t0k3n-mcp\t0k3n.exe` (Windows) and adds it to your PATH — no elevation required.

After that, updating never needs the script again:

```bash
t0k3n upgrade
```

<details>
<summary>Build from source</summary>

```bash
git clone https://github.com/tonrakun/t0k3n-mcp
cd t0k3n-mcp
cargo build --release
# → ./target/release/t0k3n
```

</details>

## Quick Start

Run in your project directory:

```bash
t0k3n setup
```

This writes (or merges into) `.mcp.json`:

```json
{
  "mcpServers": {
    "t0k3n": {
      "command": "/path/to/t0k3n",
      "args": ["--root", "/path/to/your-project"]
    }
  }
}
```

## Commands

| Command | Description |
|---------|-------------|
| `t0k3n` | Start the MCP server (stdio; MCP clients launch it with no subcommand) |
| `t0k3n upgrade` | Download the latest release and replace the binary in place |
| `t0k3n setup [dir]` | Write or merge `.mcp.json` with `--root` pinned to that directory (default: current directory) |
| `t0k3n version` | Print version |
| `t0k3n help` | Show help |

## CLI Options

| Flag | Description |
|------|-------------|
| `--root <path>` | Workspace root directory (required) |
| `--no-dashboard` | Disable the web dashboard |
| `--open-browser` | Open the dashboard in a browser on startup |
| `--dashboard-port <port>` | Dashboard port (default: 14123) |
| `--list-tools` | Print all registered tool names and exit |
| `--refresh-parsers` | Clear the tree-sitter parser cache on startup |

## Tools (67 tools)

### File Reading

| Tool | Description |
|------|-------------|
| `read_directory_tree` | `.gitignore`-aware directory tree |
| `read_markdown_toc` | Markdown heading list (TOC) |
| `read_markdown_section` | Fetch section by anchor |
| `read_code_skeleton` | Functions/classes with signatures only — no body |
| `read_code_body` | Full body for specific skeleton IDs |
| `read_code_sketch` | Control-flow sketch by ID — between skeleton and body (keeps branches/loops/calls, collapses data lines; ~60-70% smaller than the body) |
| `read_type_skeleton` | Type definitions (TS interface/type/enum, Go struct, Rust struct/enum/trait) |
| `read_call_graph` | Caller/callee graph; `depth` param for cross-file tracing |
| `read_token_map` | Files sorted by token count (glob filter) |
| `read_symbol_usages` | All usages of a symbol across the workspace |
| `read_code_deps` | import / imported_by dependency graph |
| `read_file_outline` | Auto-detects file type and returns skeleton/TOC/keys |
| `read_interface_conformance` | Find all types implementing an interface/trait (TS/Rust/Java/Kotlin/Go) |
| `search_file` | Keyword/regex search with surrounding context |
| `semantic_search` | Natural-language semantic search over code |
| `read_json_yaml_keys` | Key structure of JSON/YAML/TOML |
| `read_json_yaml_value` | Value at a dot-notation key path |
| `read_openapi` | OpenAPI/Swagger endpoint summary |
| `read_env_schema` | Environment variable definitions from .env.example / docker-compose.yml |
| `read_workspace_stats` | Codebase-wide language breakdown (files/lines/tokens) |
| `read_log_tail` | Log file tail with level filter |
| `batch_read` | Multiple read ops in one call (reduces round-trips) |

### Git

| Tool | Description |
|------|-------------|
| `read_git_diff` | Compressed git diff |
| `read_git_log` | Structured commit log (author, date, changed files) |
| `read_git_blame_body` | Per-line blame for a function range |
| `read_changed_files` | Changed files between branches with stat |
| `read_git_stash` | Stash list and diff |

### Schema / DSL

| Tool | Description |
|------|-------------|
| `read_db_schema` | Prisma / SQL schema table/model list |
| `read_db_table` | Field details for a specific table |
| `read_css_skeleton` | CSS/SCSS selector list |
| `read_css_body` | Full ruleset for specific selectors |
| `read_graphql_schema` | GraphQL type list |
| `read_graphql_type` | Field definitions for a specific type |
| `read_proto_schema` | Protocol Buffers message/service list |
| `read_proto_type` | Field/RPC definitions for a specific type |
| `read_notebook_cells` | Jupyter notebook cell list |
| `read_notebook_cell` | Full source and output of a specific cell |
| `read_test_skeleton` | Test suite/case list (Jest/pytest/Cargo/Go/JUnit/RSpec) |
| `read_test_results` | Parse test runner output into a summary |
| `read_package_manifest` | Unified dependency list from package.json/Cargo.toml/go.mod/etc. |
| `read_ci_pipeline` | GitHub Actions / GitLab CI / CircleCI workflow structure |

### Web / Document

| Tool | Description |
|------|-------------|
| `fetch_webpage` | Fetch URL → convert HTML to Markdown → return TOC |
| `read_webpage_section` | Fetch specific sections from a cached webpage |
| `convert_document` | PDF / DOCX → Markdown |

### Text / Budget

| Tool | Description |
|------|-------------|
| `compress_text` | Remove noise and excess whitespace |
| `count_tokens` | Token / character / line count |
| `check_budget` | Remaining token budget and recommended reading strategy |
| `summarize_conversation` | Summarize conversation history within a token budget |
| `read_stack_trace` | Resolve stack trace frames to source context |
| `debug_info` | Server diagnostics (version, root, DB, registered tools) |

### Memory / Task / Session

| Tool | Description |
|------|-------------|
| `memory_save/get/list/delete` | Persistent SQLite key-value store |
| `task_create/update/get/list/delete` | Task management with status, priority, tags |
| `session_snapshot/restore/list` | Save and restore work state across sessions |

### Analysis (Phase 5) — Unique to T0K3N-MCP

| Tool | Description |
|------|-------------|
| `read_complexity_map` | Cyclomatic complexity per function, risk-rated low/medium/high/critical. No compiler needed |
| `read_dead_code` | Find symbols defined but never referenced. All languages, no LSP required |
| `read_refactor_impact` | Blast-radius for a rename/refactor: callers + all referencing files + test files in one call |
| `read_security_surface` | Static security scan: injection, XSS, hardcoded secrets, unsafe code, path traversal (50 patterns) |
| `diff_schemas` | Schema diff between git refs — OpenAPI endpoints, Prisma/SQL tables, TypeScript types |
| `read_pr_context` | Full PR context in one call: changed file skeletons + related tests + commit list |

### Diagnostics (Phase 12)

| Tool | Description |
|------|-------------|
| `read_type_diagnostics` | LSP-equivalent static type diagnostics without a language server. Drives the language's own check-only engine — `cargo check` (Rust), `tsc --noEmit` (TypeScript), `pyright`/`mypy` (Python), `go vet` (Go) — and returns a compact, deduplicated `{file, line, col, severity, code, message}` list. Auto-detects the language; returns `checker_available: false` with an install hint instead of erroring when the checker is missing |

## Language Support

`read_code_skeleton`, `read_code_body`, `read_complexity_map`, and other code analysis tools support:

| Language | Extensions |
|----------|------------|
| Rust | `.rs` |
| Python | `.py` |
| JavaScript | `.js`, `.jsx` |
| TypeScript | `.ts`, `.tsx` |
| Go | `.go` |
| C++ | `.cpp`, `.cc`, `.cxx`, `.hpp` |
| Java | `.java` |
| Kotlin | `.kt` |
| Swift | `.swift` |
| Ruby | `.rb` |
| C# | `.cs` |
| PHP | `.php` |
| Lua | `.lua` |

Parsers are statically bundled at build time — no runtime downloads. New language support ships with new releases.

## Security

- All path resolution outside `--root` is blocked (path traversal protection)
- Symlink escapes beyond root are blocked
- Only web tools (`fetch_webpage`) target URLs outside root by design

---

## Data Storage

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
