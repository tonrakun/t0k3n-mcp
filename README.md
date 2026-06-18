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
| `--root <path>` | Workspace root directory (or `T0K3N_ROOT`). Optional — see below |
| `--no-dashboard` | Disable the web dashboard |
| `--open-browser` | Open the dashboard in a browser on startup |
| `--dashboard-port <port>` | Dashboard port (default: 14123) |
| `--list-tools` | Print all registered tool names and exit |
| `--refresh-parsers` | Clear the tree-sitter parser cache on startup |
| `--enable-diagnostics` | Register the opt-in `read_type_diagnostics` tool (or `T0K3N_ENABLE_DIAGNOSTICS=1`) |
| `--enable-writes` | Register the opt-in write tools — `create_file` / `delete_symbol` / `insert_symbol` / `apply_edits` / `set_config_value` / `manage_imports` / `format_code` / `move_symbol` / `edit_checkpoint` / `rollback` / `write_markdown_section` (or `T0K3N_ENABLE_WRITES=1`). Read-only by default |

### Running without a configured root

`--root` / `T0K3N_ROOT` is optional. If neither is set, the server falls back to its own
process working directory (often not the project you want) — but every tool call may pass
an extra `root` argument (an absolute path) to point the server at the right workspace for
that call. This argument is not listed in each tool's formal JSON schema (it is intercepted
before the tool's own parameters are parsed), but it is always honored when the server has
no configured root; `get_info`'s `instructions` and `debug_info`'s `root_configured` field
both surface this state to the connecting client. Once `--root` / `T0K3N_ROOT` is set, the
configured root always wins and any `root` argument on a call is ignored.

## Tools (91 tools)

### File Reading

| Tool | Description |
|------|-------------|
| `read_directory_tree` | `.gitignore`-aware directory tree |
| `read_markdown_toc` | Markdown heading list (TOC) |
| `read_markdown_section` | Fetch section by anchor |
| `read_code_skeleton` | Functions/classes with signatures only — no body |
| `read_code_body` | Full body for specific skeleton IDs. `zoom` selects detail (`body`/`sketch`/`skeleton`/`auto`); `auto` follows the latest `check_budget` strategy (critical→skeleton, aggressive→sketch) |
| `read_code_sketch` | Control-flow sketch by ID — between skeleton and body (keeps branches/loops/calls, collapses data lines; ~60-70% smaller than the body) |
| `rename_symbol` | Rename a symbol workspace-wide in one call — write counterpart of `read_symbol_usages`. Whole-identifier match; returns affected files + per-line before/after only (`dry_run` to preview) |
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
| `batch_read` | Multiple read ops in one call (reduces round-trips). `factor:true` collapses near-identical results (migrations/fixtures) into one template + per-file diffs |

### Git

| Tool | Description |
|------|-------------|
| `read_git_diff` | Compressed git diff (`zoom`: skeleton/sketch/body/auto — structural change summary) |
| `read_git_log` | Structured commit log (author, date, changed files) |
| `read_git_blame_body` | Per-line blame for a function range |
| `read_changed_files` | Changed files between branches with stat |
| `read_git_stash` | Stash list and diff |
| `read_code_ownership` | Fuses `git log` + blame: per file, churn (commit count), last-modified date, and top authors by lines contributed. Sorted by churn to surface hotspots |

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
| `read_test_coverage` | Map a coverage report (lcov / coverage.py JSON / cobertura) onto symbols — per-function covered/total/pct to spot untested code. `uncovered_only` / `threshold` filters |
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
| `read_type_diagnostics` | **Opt-in** (`--enable-diagnostics` or `T0K3N_ENABLE_DIAGNOSTICS=1`; off by default since it spawns the toolchain). LSP-equivalent static type diagnostics without a language server. Drives the language's own check-only engine — `cargo check` (Rust), `tsc --noEmit` (TypeScript), `pyright`/`mypy` (Python), `go vet` (Go) — and returns a compact, deduplicated `{file, line, col, severity, code, message}` list. Auto-detects the language; returns `checker_available: false` with an install hint instead of erroring when the checker is missing |
| `project_digest` | Warm-start architecture summary in one call: git HEAD, language stats, entry-point files with their top symbols, and a shallow directory tree (~2k tokens). Cached in `.t0k3n/digest.json` and auto-invalidated when HEAD changes — replaces the repeated tree → stats → skeleton exploration at session start |

### Security & API (Phase 13)

| Tool | Description |
|------|-------------|
| `read_dependency_audit` | Dependency-side counterpart to `read_security_surface`. Auto-detects the ecosystem (Cargo.toml→`cargo audit`, package.json→`npm audit`, pyproject/requirements→`pip-audit`, go.mod→`osv-scanner`) and normalizes results to `{package, severity, id, affected, patched, title}`, sorted by severity. Returns `scanner_available: false` with an install hint when the scanner is missing |
| `read_api_surface` | Extract only the public API surface — Rust `pub` items, TS/JS `export`s, Python `__all__` / non-underscore top-level defs, Go capitalized identifiers. Signatures only. Pairs with `diff_schemas` to detect breaking changes (`include_crate_visible` also lists Rust `pub(crate)`) |

## MCP Resources

Key workspace files (manifests, READMEs, conventional entry points) are exposed as MCP resources under the `t0k3n://<path>` URI scheme, so resource-aware clients can list and read them via the standard `resources/list` and `resources/read` methods. URIs are resolved through the same path-traversal guard as the file tools.

## Cross-session delta (gen4)

The cross-tool content ledger is persisted to `.t0k3n/content_ledger.json` and survives across sessions. A body that is unchanged since a previous session (verified by mtime + content hash) returns a clearly-labeled cold-cache stub — it is **not** falsely reported as already in the current context. `delta_reset` clears the persisted ledger.

## Write tools (Phase 14–15, 18, opt-in)

T0K3N-MCP is read-first. Mutating tools are **off by default** and only registered with `--enable-writes` (or `T0K3N_ENABLE_WRITES=1`), so the server is safe to point at any repo until you opt in. They share the house rules: `dry_run` preview, stale-line guards, CRLF/newline preservation, and diff/summary-only output (never the full file body). (`patch_symbol` and `rename_symbol` predate the gate and stay always-on.)

| Tool | Description |
|------|-------------|
| `create_file` | Create a new file. Refuses to overwrite unless `overwrite:true`; makes parent dirs. Fills the gap where the only way to create a file was `run_command` |
| `delete_symbol` | Delete a symbol by skeleton ID — write counterpart of `read_dead_code`. Removes the range plus one trailing blank line; `expected_name` guards stale line numbers |
| `insert_symbol` | Insert code at a structural location: `after_symbol` / `before_symbol` (by skeleton ID), `after_imports`, or `end_of_file`. Completes symbol CRUD with `patch_symbol` (update) and `delete_symbol` (delete) |
| `apply_edits` | Atomic multi-file find/replace — write counterpart of `batch_read`. Each find must match once per file; if any edit fails, nothing is written |
| `set_config_value` | Set a JSON/YAML/TOML value by dot-path — write counterpart of `read_json_yaml_value`. Creates intermediate objects; preserves JSON key order |
| `manage_imports` | Add/remove import statements (language-agnostic, whole-line). Inserts at the import block, removes by trimmed match, de-duplicates |
| `format_code` | Run the language formatter (rustfmt/prettier/black/gofmt) on a file. Returns the diff; non-error install hint if the formatter is missing |
| `move_symbol` | Move a symbol to another file (created if missing). Import fixups are best-effort — referencing files are reported in warnings |
| `edit_checkpoint` / `rollback` | Snapshot the working tree before a batch of edits and restore it on failure. Uses `git stash create` in a repo, else a gitignore-aware copy. A safety net for autonomous write loops |
| `write_markdown_section` | Edit a Markdown section by heading anchor — write counterpart of `read_markdown_toc` / `read_markdown_section`. `mode`: `replace` / `insert_before` / `insert_after` / `append` / `delete`; `expected_title` guards a stale TOC |

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
