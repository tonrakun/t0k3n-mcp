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

## Tools (67 tools)

### File Reading

| Tool | Description |
|------|-------------|
| `read_directory_tree` | `.gitignore`-aware directory tree |
| `read_markdown_toc` | Markdown heading list (TOC) |
| `read_markdown_section` | Fetch section by anchor |
| `read_code_skeleton` | Functions/classes with signatures only — no body |
| `read_code_body` | Full body for specific skeleton IDs |
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
