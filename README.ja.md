# T0K3N-MCP

> **AI コーディングツールのトークン消費を 87% 削減する MCP サーバー**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![Token Savings](https://img.shields.io/badge/Token%20Savings-87.3%25-brightgreen)](.docs/benchmark_token_savings.md)

[English](README.en.md) | **日本語** | [中文](README.zh.md) | [한국어](README.ko.md)

---

## 実測: 75〜87% のトークンを節約する

Anthropic の公式トークンカウント API を使い、**2 種類の実プロジェクト**で計測しました。

### Study 1: Rust プロジェクト（T0K3N-MCP 自身）

| ファイル | フル | T0K3N-MCP | 削減率 |
|---------|------|-----------|--------|
| `code.rs` (295行) | 3,642 | 345 | **90.5%** |
| `mod.rs` (422行) | 4,997 | 1,162 | **76.7%** |
| `README.md` | 2,492 | 296 | **88.1%** |
| `Cargo.toml` | 491 | 24 | **95.1%** |
| **平均** | 2,147 | 321 | **87.3%** |

### Study 2: Next.js プロジェクト（vercel/commerce）

| ファイル | フル | T0K3N-MCP | 削減率 |
|---------|------|-----------|--------|
| `components/cart/modal.tsx` | 2,776 | 143 | **94.8%** |
| `app/product/[handle]/page.tsx` | 1,400 | 134 | **90.4%** |
| `lib/shopify/index.ts` | 4,073 | 1,299 | **68.1%** |
| `components/cart/cart-context.tsx` | 1,742 | 488 | **72.0%** |
| **平均 (20ファイル)** | 957 | 198 | **75.5%** |

### プロジェクト全体シミュレーション（5タスク調査）

| | 標準 | T0K3N-MCP | 削減率 |
|-|------|-----------|--------|
| Next.js 調査 | 19,109 tokens | 2,668 tokens | **86.0%** |

> 測定方法・全データは [`.docs/benchmark_token_savings.md`](.docs/benchmark_token_savings.md) を参照

200,000 トークンのコンテキストウィンドウが、**実質 6〜8 倍**に広がります。

---

## なぜ標準ツールでは足りないのか

Claude Code や Cursor の標準 Read File は、ファイルをそのままコンテキストに流し込みます。

```
read_file("server/mod.rs")  →  4,997 トークン消費
                                ↑ その95%は今の質問と無関係
```

T0K3N-MCP は **「構造を先に取得し、必要な部分だけを取得する」** 設計でこれを解決します。

```
read_code_skeleton("server/mod.rs")  →  1,162 トークン（シグネチャのみ）
read_code_body(["function:54-67"])   →    150 トークン（対象関数のみ）
                                         ────────────────────────────
合計                                       1,312 トークン  ← 74% 削減
```

---

## インストール

### ビルド済みバイナリ（推奨）

GitHub Releases からお使いの OS のバイナリをダウンロードしてください。

| OS | ファイル |
|----|---------|
| macOS (Apple Silicon) | `t0k3n-mcp-macos-aarch64` |
| macOS (Intel) | `t0k3n-mcp-macos-x86_64` |
| Linux x86_64 | `t0k3n-mcp-linux-x86_64` |
| Linux ARM64 | `t0k3n-mcp-linux-aarch64` |
| Windows x86_64 | `t0k3n-mcp-windows-x86_64.exe` |

### ソースからビルド

```bash
git clone https://github.com/tonrakun/t0k3n-mcp
cd t0k3n-mcp
cargo build --release
# → ./target/release/t0k3n-mcp
```

Rust 以外の依存はありません。Node.js / npm / Python 不要。

---

## セットアップ

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

同じ設定を各クライアントの MCP 設定ファイルに追加するだけです。

### オプション

```
--root <path>          ワークスペースルート（必須）
--refresh-parsers      パーサーキャッシュをクリアして再ダウンロード
```

---

## 使い方

### コードファイル（Rust / Python / JS / TS / Go）

```
1. read_code_skeleton("path/to/file.rs")
   → 関数・struct・impl のシグネチャ一覧 + ID を返す

2. read_code_body(["function:10-45", "impl:87-130"])
   → 指定した関数だけの本文を返す
```

### Markdown / ドキュメント

```
1. read_markdown_toc("ARCHITECTURE.md")
   → 見出し一覧（anchor 付き）を返す

2. read_markdown_section("ARCHITECTURE.md", ["#データベース設計"])
   → 指定セクションだけを返す
```

### Web ページ

```
1. fetch_webpage("https://docs.rs/tokio/latest/tokio/")
   → HTML を Markdown 変換し TOC のみ返す

2. read_webpage_section(url, ["#struct-JoinHandle"])
   → キャッシュ済み MD から指定セクションを返す
```

### PDF / DOCX

```
1. convert_document("report.pdf")
   → Markdown に変換し TOC と tmp_path を返す

2. read_markdown_section(tmp_path, ["#第3章"])
   → 指定セクションだけを返す
```

### トークンバジェット管理

```
1. check_budget(budget=8000, candidates=["a.rs", "b.rs", "c.md"])
   → strategy: "full" | "skeleton_only" | "toc_only" | "skip"

2. 戦略に応じてツールを選択
```

---

## ツール一覧（26 ツール）

### ファイル読み取り

| ツール | 説明 |
|--------|------|
| `read_directory_tree` | `.gitignore` 適用済みのディレクトリツリー |
| `read_markdown_toc` | Markdown 見出し一覧（TOC） |
| `read_markdown_section` | anchor 指定でセクション本文取得 |
| `read_code_skeleton` | 関数・クラス一覧をシグネチャのみで返す |
| `read_code_body` | skeleton の ID 指定で関数本文取得 |
| `search_file` | キーワード/regex マッチ行と前後文脈 |
| `read_json_yaml_keys` | JSON/YAML のキー構造一覧 |
| `read_json_yaml_value` | ドット記法キーパスで値取得 |

### Web・ドキュメント

| ツール | 説明 |
|--------|------|
| `fetch_webpage` | HTML → Markdown 変換・圧縮 → TOC |
| `read_webpage_section` | キャッシュ済み Web ページのセクション取得 |
| `convert_document` | PDF / DOCX → Markdown 変換 |

### テキスト・バジェット

| ツール | 説明 |
|--------|------|
| `compress_text` | Markdown ノイズ・余分な空白を除去 |
| `count_tokens` | トークン数・文字数・行数カウント |
| `check_budget` | 残量と推奨読み取り戦略を返す |
| `summarize_conversation` | 会話履歴を指定トークン予算内に要約 |

### 記憶 / タスク / セッション

| ツール | 説明 |
|--------|------|
| `memory_save/get/list/delete` | SQLite 永続キーバリューストア |
| `task_create/update/get/list/delete` | タスク管理（状態・優先度・タグ） |
| `session_snapshot/restore/list` | 作業状態の保存と復元 |

---

## セキュリティ

- `--root` 外へのパス解決を全ブロック（パストラバーサル対策）
- シンボリックリンクによる root 外エスケープをブロック
- Web ツール（`fetch_webpage`）のみ root 外 URL を対象（設計上）

---

## データ保存先

```
<root>/.t0k3n/
  t0k3n.db        ← SQLite（記憶・タスク・セッション）

~/.cache/t0k3n-mcp/
  parsers/        ← 言語パーサーキャッシュ（Phase 3）
```

`.gitignore` への追加を推奨します：

```gitignore
.t0k3n/
```

---

## ライセンス

[MIT](LICENSE) © 2025 Tonrakun
