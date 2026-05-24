# T0K3N-MCP

> AI コーディングツール向けトークン節約特化型 MCP サーバー

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust-orange.svg)](https://www.rust-lang.org/)

---

## なぜ T0K3N-MCP が必要か

Claude Code 等の AI コーディングツールは、標準の Read File でファイルをそのままコンテキストに流し込みます。`package-lock.json` 1 ファイルだけで 88,000 トークン超を消費することもあります。

T0K3N-MCP は **「構造を先に取得し、必要な部分だけを取得する」** 設計で、これを解決します。

- コードファイルは **スケルトン（シグネチャのみ）→ 必要な関数だけ本文取得**
- Markdown は **目次だけ → 必要なセクションだけ取得**
- Web ページ・PDF・DOCX も **同じフローに統一**
- トークンバジェット管理で **どこまで読めるか戦略的に判断**

単一 Rust バイナリで動作。Node.js / npm 不要。

---

## インストール

### ビルド済みバイナリ（推奨）

```bash
# macOS / Linux
curl -fsSL https://github.com/your-org/t0k3n-mcp/releases/latest/download/install.sh | sh

# Windows (PowerShell)
irm https://github.com/tonrakun/t0k3n-mcp/releases/latest/download/install.ps1 | iex
```

### ソースからビルド

```bash
git clone https://github.com/tonrakun/t0k3n-mcp
cd t0k3n-mcp
cargo build --release
# バイナリ: ./target/release/t0k3n-mcp
```

---

## セットアップ

### Claude Code (`.mcp.json`)

```json
{
  "mcpServers": {
    "t0k3n": {
      "command": "t0k3n-mcp",
      "args": ["--root", "/path/to/your/project"]
    }
  }
}
```

### Cursor / Cline / Codex

```json
{
  "mcpServers": {
    "t0k3n": {
      "command": "t0k3n-mcp",
      "args": ["--root", "/path/to/your/project"]
    }
  }
}
```

### 起動時の動作

`--root` で指定したワークスペース内の言語を自動判別し、対応する tree-sitter パーサーを `~/.cache/t0k3n-mcp/parsers/` にダウンロードします。2 回目以降はキャッシュを再利用します。パーサーのダウンロード中もツールは使用可能です。

---

## 使い方

### コードファイルの読み取り

```
1. read_code_skeleton  → 関数・クラスのシグネチャ一覧を取得
2. 必要な関数の ID を特定
3. read_code_body      → 該当関数の本文だけを取得
```

### Markdown ファイルの読み取り

```
1. read_markdown_toc     → 目次（見出し一覧）を取得
2. 必要なセクションの anchor を特定
3. read_markdown_section → 該当セクションだけを取得
```

### Web ページの読み取り

```
1. fetch_webpage         → HTML を MD 変換し TOC を取得
2. 必要なセクションの anchor を特定
3. read_webpage_section  → 該当セクションだけを取得
```

### PDF / DOCX の読み取り

```
1. convert_document      → MD に変換し TOC を取得（tmp_path を返す）
2. 必要なセクションの anchor を特定
3. read_markdown_section(tmp_path, anchors) → 該当セクションだけを取得
```

### トークンバジェット管理

```
1. check_budget(budget, candidates) → 残量と推奨戦略を取得
   strategy: "full" | "skeleton_only" | "toc_only" | "skip"
2. 戦略に応じてツールを選択
```

---

## ツール一覧

### ファイル読み取り系

| ツール | 説明 |
|---|---|
| `read_directory_tree` | `.gitignore` 適用済みのディレクトリツリーを返す |
| `read_markdown_toc` | Markdown の見出し一覧（目次）を返す |
| `read_markdown_section` | anchor 指定でセクション本文を返す |
| `read_code_skeleton` | 関数・クラス一覧をシグネチャのみで返す（tree-sitter AST） |
| `read_code_body` | スケルトンの ID 指定で関数本文を返す |
| `read_git_diff` | 圧縮済みの git diff を返す |
| `search_file` | キーワードマッチ行と前後の文脈を返す |
| `semantic_search` | 自然言語クエリで関連関数・クラスを検索して本文を返す |
| `read_json_yaml_keys` | JSON/YAML のキー構造一覧を返す |
| `read_json_yaml_value` | 指定キーパスの値を返す |

### Web 取得系

| ツール | 説明 |
|---|---|
| `fetch_webpage` | Web ページを MD 変換・圧縮して TOC を返す |
| `read_webpage_section` | `fetch_webpage` でキャッシュされた MD から指定セクションを返す |

### ドキュメント変換系

| ツール | 説明 |
|---|---|
| `convert_document` | PDF / DOCX を MD に変換し TOC と一時ファイルパスを返す |

### テキスト圧縮系

| ツール | 説明 |
|---|---|
| `compress_text` | Markdown ノイズ・重複行・余分な空白を除去して圧縮する |

### コンテキスト管理系

| ツール | 説明 |
|---|---|
| `count_tokens` | テキストのトークン数を返す（近似値） |
| `summarize_conversation` | 会話履歴を要約して圧縮する |
| `check_budget` | トークン残量と推奨読み取り戦略を返す |

### 記憶系

| ツール | 説明 |
|---|---|
| `memory_save` | キーと値を永続保存する |
| `memory_get` | キーで値を取得する |
| `memory_list` | タグ・キーワードで記憶一覧を取得する |
| `memory_delete` | キーで記憶を削除する |

### タスク系

| ツール | 説明 |
|---|---|
| `task_create` | タスクを作成する |
| `task_update` | タスクのステータスとメモを更新する |
| `task_get` | タスク ID で詳細を取得する |
| `task_list` | ステータスでフィルタしてタスク一覧を取得する |
| `task_delete` | タスクを削除する |

### セッション系

| ツール | 説明 |
|---|---|
| `session_snapshot` | 現在の作業状態をスナップショットとして保存する |
| `session_restore` | 保存済みスナップショットを復元する |
| `session_list` | スナップショット一覧を返す |

---

## データ保存

```
.t0k3n/               ← プロジェクトルート（.gitignore 追加推奨）
  t0k3n.db            ← SQLite（記憶・タスク・セッション）

~/.cache/t0k3n-mcp/
  parsers/
    tree-sitter-rust/0.21.0/
    tree-sitter-python/0.21.0/
    ...               ← 検出言語を自動ダウンロード・無制限
```

`.gitignore` への追加を推奨します：

```gitignore
.t0k3n/
```

---

## セキュリティ

- `--root` で指定したディレクトリ外へのパス解決は禁止（パストラバーサル対策）
- シンボリックリンクの root 外への追跡は禁止
- `fetch_webpage` / `read_webpage_section` は外部 URL を対象とするため root 制約の適用外