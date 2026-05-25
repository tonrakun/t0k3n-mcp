# T0K3N-MCP

> **将 AI 编码工具的 Token 消耗减少高达 87% 的 MCP 服务器**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![Token Savings](https://img.shields.io/badge/Token%20Savings-87.3%25-brightgreen)](.docs/benchmark_token_savings.md)

[English](README.en.md) | [日本語](README.ja.md) | **中文** | [한국어](README.ko.md)

---

## 基准测试：节省 75–87% 的 Token

使用 Anthropic 官方 token 计数 API 对**两个真实项目**进行了测量。

### 研究 1：Rust 项目（T0K3N-MCP 本身）

| 文件 | 完整 | T0K3N-MCP | 节省率 |
|------|------|-----------|--------|
| `code.rs`（295 行） | 3,642 | 345 | **90.5%** |
| `mod.rs`（422 行） | 4,997 | 1,162 | **76.7%** |
| `README.md` | 2,492 | 296 | **88.1%** |
| `Cargo.toml` | 491 | 24 | **95.1%** |
| **平均** | 2,147 | 321 | **87.3%** |

### 研究 2：Next.js 项目（vercel/commerce）

| 文件 | 完整 | T0K3N-MCP | 节省率 |
|------|------|-----------|--------|
| `components/cart/modal.tsx` | 2,776 | 143 | **94.8%** |
| `app/product/[handle]/page.tsx` | 1,400 | 134 | **90.4%** |
| `lib/shopify/index.ts` | 4,073 | 1,299 | **68.1%** |
| `components/cart/cart-context.tsx` | 1,742 | 488 | **72.0%** |
| **平均（20 个文件）** | 957 | 198 | **75.5%** |

### 完整项目模拟（5 个任务调查）

| | 标准 | T0K3N-MCP | 节省率 |
|-|------|-----------|--------|
| Next.js 调查 | 19,109 tokens | 2,668 tokens | **86.0%** |

> 完整方法论与数据：[`.docs/benchmark_token_savings.md`](.docs/benchmark_token_savings.md)

200,000 token 的上下文窗口实际上可以扩大 **6–8 倍**。

---

## 为什么标准工具不够用

Claude Code 和 Cursor 的标准 Read File 会将整个文件倾倒进上下文：

```
read_file("server/mod.rs")  →  消耗 4,997 个 token
                                ↑ 其中 95% 与当前问题无关
```

T0K3N-MCP 通过**「先获取结构，再按需获取内容」**的设计来解决这个问题：

```
read_code_skeleton("server/mod.rs")  →  1,162 tokens（仅签名）
read_code_body(["function:54-67"])   →    150 tokens（仅目标函数）
                                         ────────────────────────
合计                                       1,312 tokens  ← 节省 74%
```

---

## 安装

### 预构建二进制文件（推荐）

从 GitHub Releases 下载适合您系统的二进制文件。

| 操作系统 | 文件 |
|---------|------|
| macOS (Apple Silicon) | `t0k3n-mcp-macos-aarch64` |
| macOS (Intel) | `t0k3n-mcp-macos-x86_64` |
| Linux x86_64 | `t0k3n-mcp-linux-x86_64` |
| Linux ARM64 | `t0k3n-mcp-linux-aarch64` |
| Windows x86_64 | `t0k3n-mcp-windows-x86_64.exe` |

### 从源码构建

```bash
git clone https://github.com/tonrakun/t0k3n-mcp
cd t0k3n-mcp
cargo build --release
# → ./target/release/t0k3n-mcp
```

除 Rust 外无其他依赖，不需要 Node.js、npm 或 Python。

---

## 配置

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

将相同配置添加到各客户端的 MCP 设置文件中即可。

### 选项

```
--root <path>          工作区根目录（必填）
--refresh-parsers      清除并重新下载解析器缓存
```

---

## 使用方法

### 代码文件（Rust / Python / JS / TS / Go）

```
1. read_code_skeleton("path/to/file.rs")
   → 返回函数 / struct / impl 签名列表 + ID

2. read_code_body(["function:10-45", "impl:87-130"])
   → 返回指定函数的函数体
```

### Markdown / 文档

```
1. read_markdown_toc("ARCHITECTURE.md")
   → 返回带锚点的标题列表

2. read_markdown_section("ARCHITECTURE.md", ["#database-design"])
   → 仅返回指定章节
```

### 网页

```
1. fetch_webpage("https://docs.rs/tokio/latest/tokio/")
   → 将 HTML 转换为 Markdown 并返回目录

2. read_webpage_section(url, ["#struct-JoinHandle"])
   → 从缓存的 Markdown 中返回指定章节
```

### PDF / DOCX

```
1. convert_document("report.pdf")
   → 转换为 Markdown 并返回目录 + tmp_path

2. read_markdown_section(tmp_path, ["#chapter-3"])
   → 仅返回指定章节
```

### Token 预算管理

```
1. check_budget(budget=8000, candidates=["a.rs", "b.rs", "c.md"])
   → strategy: "full" | "skeleton_only" | "toc_only" | "skip"

2. 根据推荐策略选择工具
```

---

## 工具参考（26 个工具）

### 文件读取

| 工具 | 描述 |
|------|------|
| `read_directory_tree` | 带 `.gitignore` 过滤的目录树 |
| `read_markdown_toc` | Markdown 标题列表（目录） |
| `read_markdown_section` | 通过锚点获取章节内容 |
| `read_code_skeleton` | 仅返回函数 / 类签名 |
| `read_code_body` | 通过 skeleton ID 获取函数体 |
| `search_file` | 关键字 / 正则匹配及上下文 |
| `read_json_yaml_keys` | 列出 JSON/YAML 的键结构 |
| `read_json_yaml_value` | 通过点分键路径获取值 |

### 网页与文档

| 工具 | 描述 |
|------|------|
| `fetch_webpage` | HTML → Markdown 转换 + 压缩 → 目录 |
| `read_webpage_section` | 从缓存网页中获取章节 |
| `convert_document` | PDF / DOCX → Markdown 转换 |

### 文本与预算

| 工具 | 描述 |
|------|------|
| `compress_text` | 去除 Markdown 噪音和多余空白 |
| `count_tokens` | Token / 字符 / 行数统计 |
| `check_budget` | 返回剩余预算和推荐读取策略 |
| `summarize_conversation` | 在 token 预算内摘要对话历史 |

### 记忆 / 任务 / 会话

| 工具 | 描述 |
|------|------|
| `memory_save/get/list/delete` | SQLite 持久化键值存储 |
| `task_create/update/get/list/delete` | 任务管理（状态、优先级、标签） |
| `session_snapshot/restore/list` | 保存和恢复工作状态 |

---

## 安全性

- 阻止所有 `--root` 外的路径解析（路径遍历防护）
- 阻止符号链接逃逸到 root 外
- 仅 Web 工具（`fetch_webpage`）可访问 root 外的 URL（符合设计意图）

---

## 数据存储

```
<root>/.t0k3n/
  t0k3n.db        ← SQLite（记忆、任务、会话）

~/.cache/t0k3n-mcp/
  parsers/        ← 语言解析器缓存（Phase 3）
```

推荐在 `.gitignore` 中添加：

```gitignore
.t0k3n/
```

---

## 许可证

[MIT](LICENSE) © 2025 Tonrakun
