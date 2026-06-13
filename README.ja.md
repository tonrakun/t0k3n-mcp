# T0K3N-MCP

> **AI コーディングツールのトークン消費を 87% 削減する MCP サーバー**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![Token Savings](https://img.shields.io/badge/Token%20Savings-87.3%25-brightgreen)](.docs/benchmark_token_savings.md)

[English](README.md) | **日本語**

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

**macOS / Linux**

```bash
curl -fsSL https://raw.githubusercontent.com/tonrakun/t0k3n-mcp/main/install.sh | bash
```

**Windows（PowerShell）**

```powershell
irm https://raw.githubusercontent.com/tonrakun/t0k3n-mcp/main/install.ps1 | iex
```

Unix は `~/.t0k3n-mcp/t0k3n`、Windows は `%USERPROFILE%\t0k3n-mcp\t0k3n.exe` にインストールされ、PATH にも追加されます（管理者権限は不要）。

2回目以降の更新はスクリプト不要です：

```bash
t0k3n upgrade
```

<details>
<summary>ソースからビルド</summary>

```bash
git clone https://github.com/tonrakun/t0k3n-mcp
cd t0k3n-mcp
cargo build --release
# → ./target/release/t0k3n
```

Rust 以外の依存はありません。Node.js / npm / Python 不要。

</details>

---

## セットアップ

### Claude Code (`.mcp.json`)

プロジェクトディレクトリで実行するだけです：

```bash
t0k3n setup
```

`.mcp.json` が生成（既存の場合はマージ）されます：

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

### Cursor / Cline / Windsurf

同じ設定を各クライアントの MCP 設定ファイルに追加するだけです。

### コマンド

| コマンド | 説明 |
|----------|------|
| `t0k3n` | MCP サーバーを起動（stdio、引数なしがデフォルト） |
| `t0k3n upgrade` | 最新リリースをダウンロードしてその場で自己更新 |
| `t0k3n setup [dir]` | `.mcp.json` を生成・マージし、そのディレクトリを `--root` に設定（デフォルト: カレントディレクトリ） |
| `t0k3n version` | バージョンを表示 |
| `t0k3n help` | ヘルプを表示 |

### オプション

| フラグ | 説明 |
|--------|------|
| `--root <path>` | ワークスペースルート（必須） |
| `--no-dashboard` | Web ダッシュボードを無効化 |
| `--open-browser` | 起動時にダッシュボードをブラウザで開く |
| `--dashboard-port <port>` | ダッシュボードのポート番号（デフォルト: 14123） |
| `--list-tools` | 登録済みツール一覧を表示して終了 |
| `--refresh-parsers` | 起動時に tree-sitter パーサーキャッシュをクリア |

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

## ツール一覧（67 ツール）

### ファイル読み取り

| ツール | 説明 |
|--------|------|
| `read_directory_tree` | `.gitignore` 適用済みのディレクトリツリー |
| `read_markdown_toc` | Markdown 見出し一覧（TOC） |
| `read_markdown_section` | anchor 指定でセクション本文取得 |
| `read_code_skeleton` | 関数・クラス一覧をシグネチャのみで返す |
| `read_code_body` | skeleton の ID 指定で関数本文取得 |
| `read_code_sketch` | skeleton と body の中間ズーム。ID 指定で制御フロー骨格（分岐/ループ/呼び出しを残し純データ行を畳む。body 比 60〜70% 削減） |
| `read_type_skeleton` | 型定義スケルトン（TS interface/type/enum・Go struct/interface・Rust struct/enum/trait） |
| `read_call_graph` | 関数の呼び出し先・呼び出し元グラフ（depth 指定でクロスファイル対応） |
| `read_token_map` | ワークスペース内ファイルのトークン数マップ（glob フィルタ・降順ソート） |
| `read_symbol_usages` | ワークスペース全体のシンボル使用箇所を検索 |
| `read_code_deps` | import / imported_by 依存グラフ |
| `read_file_outline` | ファイル種別自動判別の統合アウトライン |
| `read_interface_conformance` | interface / trait の実装型をワークスペース全体から検索（TS/Rust/Java/Kotlin/Go） |
| `search_file` | キーワード/regex マッチ行と前後文脈 |
| `semantic_search` | 自然言語で意味的に近い関数を検索 |
| `read_json_yaml_keys` | JSON/YAML/TOML のキー構造一覧 |
| `read_json_yaml_value` | ドット記法キーパスで値取得（JSON/YAML/TOML） |
| `read_openapi` | OpenAPI/Swagger エンドポイント一覧取得 |
| `read_env_schema` | .env.example / docker-compose.yml から環境変数定義を抽出 |
| `read_workspace_stats` | コードベース全体の言語別統計（ファイル数・行数・トークン数） |
| `read_log_tail` | ログファイル末尾のN行取得（ログレベル別カウント付き） |
| `batch_read` | 複数の読み取り操作を 1 コールで並列実行（ラウンドトリップ削減） |

### Git

| ツール | 説明 |
|--------|------|
| `read_git_diff` | 圧縮済み git diff |
| `read_git_log` | 構造化コミットログ（著者・日付・変更ファイル） |
| `read_git_blame_body` | 関数単位の行 blame（著者・日付） |
| `read_changed_files` | ブランチ間の変更ファイル一覧（ステータス・追加/削除行数） |
| `read_git_stash` | スタッシュ一覧と diff 取得 |

### DB スキーマ

| ツール | 説明 |
|--------|------|
| `read_db_schema` | Prisma / SQL スキーマのテーブル/モデル一覧（自動検出対応） |
| `read_db_table` | テーブル/モデルのフィールド定義詳細取得 |

### CSS

| ツール | 説明 |
|--------|------|
| `read_css_skeleton` | CSS/SCSS セレクタ一覧（プロパティ数・行範囲） |
| `read_css_body` | セレクタ ID 指定でルールセット本文取得 |

### GraphQL

| ツール | 説明 |
|--------|------|
| `read_graphql_schema` | GraphQL スキーマの型一覧（type/input/enum/interface） |
| `read_graphql_type` | 型名指定でフィールド定義詳細取得 |

### Proto

| ツール | 説明 |
|--------|------|
| `read_proto_schema` | Protocol Buffers スキーマの型/サービス一覧（message/enum/service） |
| `read_proto_type` | メッセージ/サービス名指定でフィールド・RPC 定義取得 |

### Notebook

| ツール | 説明 |
|--------|------|
| `read_notebook_cells` | Jupyter Notebook のセル一覧（タイプ・ソース・行数） |
| `read_notebook_cell` | セル番号指定で本文・出力取得 |

### テスト

| ツール | 説明 |
|--------|------|
| `read_test_skeleton` | テストファイルのスイート/テスト一覧（Jest/pytest/Cargo/Go/JUnit/RSpec） |
| `read_test_results` | テスト結果テキストのパース・サマリ返却（フレームワーク自動検出） |

### パッケージ・CI

| ツール | 説明 |
|--------|------|
| `read_package_manifest` | package.json / Cargo.toml / go.mod 等を統一フォーマットで返す |
| `read_ci_pipeline` | GitHub Actions / GitLab CI / CircleCI ワークフロー構造取得 |

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
| `read_stack_trace` | スタックトレース解析（フレーム・ファイル・行・前後コード付き） |
| `debug_info` | サーバー診断（バージョン・root・DB状態・登録ツール一覧） |

### 記憶 / タスク / セッション

| ツール | 説明 |
|--------|------|
| `memory_save/get/list/delete` | SQLite 永続キーバリューストア |
| `task_create/update/get/list/delete` | タスク管理（状態・優先度・タグ） |
| `session_snapshot/restore/list` | 作業状態の保存と復元 |

### 分析系（Phase 5）

他の MCP サーバーが持たない差別化ツール群です。

| ツール | 説明 |
|--------|------|
| `read_complexity_map` | 関数ごとの循環的複雑度を計算し low / medium / high / critical でリスク分類。コンパイラ不要 |
| `read_dead_code` | 定義されているが参照ゼロのシンボルを検出。全言語対応・LSP 不要 |
| `read_refactor_impact` | シンボル名 1 つで「呼び出し元・全参照ファイル・テストファイル・ブラスト半径」を 1 コールで返す |
| `read_security_surface` | injection / XSS / hardcoded secrets / unsafe / path_traversal を 50 パターンで静的スキャン |
| `diff_schemas` | OpenAPI・Prisma/SQL・TypeScript 型を git ref 間で比較し added / removed / modified を返す |
| `read_pr_context` | branch + base 指定で変更ファイルのスケルトン・関連テスト・コミット一覧を 1 コールでロード |

---

### 診断系（Phase 12）

| ツール | 説明 |
|--------|------|
| `read_type_diagnostics` | 言語サーバー常駐なしで LSP 相当の静的型診断を取得。各言語の check-only エンジン（`cargo check` / `tsc --noEmit` / `pyright`・`mypy` / `go vet`）を駆動し、重複排除済みの `{file, line, col, severity, code, message}` を返す。言語自動判別・チェッカー未導入時は `checker_available: false` + インストールヒントで非エラー応答 |

---

## 対応言語

`read_code_skeleton` / `read_code_body` / `read_code_deps` / `read_complexity_map` 等が対応するコード解析言語：

| 言語 | 拡張子 |
|------|--------|
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

パーサーは Cargo クレートとしてビルド時にバイナリへ静的に組み込まれています。新言語の追加は新リリースで提供されます。リクエストは [GitHub Issues](https://github.com/tonrakun/t0k3n-mcp/issues) へ。

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
```

`.gitignore` への追加を推奨します：

```gitignore
.t0k3n/
```

---

## ライセンス

[MIT](LICENSE) © 2025 Tonrakun
