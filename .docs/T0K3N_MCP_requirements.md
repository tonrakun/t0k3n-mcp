# T0K3N-MCP 要件定義書

> **参考**: [Sophon-MCP](https://github.com/tonrakun/Sophon-MCP)（機能設計の参考のみ）  
> **目的**: AI コーディングツール向けトークン節約特化型汎用 MCP サーバー

---

## 1. プロジェクト概要

### 1.1 コンセプト

「**構造を先に取得し、必要な部分だけを取得する**」設計思想のもと、以下を実現する。

- 単一 Rust バイナリとして動作（Node.js / npm 不要）
- 多言語コード解析（tree-sitter による AST パース）
- ドキュメント変換（PDF / DOCX → MD）
- Web 取得の段階的読み取り（HTML → MD → TOC → セクション選択）
- トークンバジェット管理
- 差分取得

> Sophon-MCP は機能設計の参考実装として参照する。コード・依存関係は引き継がない。

### 1.2 対象クライアント

- Claude Code（主ターゲット）
- Cursor / Cline / Codex（互換対応）

### 1.3 技術スタック

| レイヤー | 技術 | 理由 |
|---|---|---|
| MCPプロトコル / ツール定義 | Rust（`rmcp` クレート） | コア統一・低レイテンシ |
| コード構文解析 | tree-sitter（Rust ネイティブバインディング） | 多言語・高精度 AST |
| HTML→MD変換 | `htmd` クレート | 外部プロセス不要 |
| PDF 変換 | `pdf-extract` クレート | Rust ネイティブ |
| DOCX 変換 | `docx-rs` クレート | Rust ネイティブ |
| DB | SQLite（`rusqlite`） | 軽量・組み込み |
| スクリプト補助 | Deno（必要に応じて） | 設定ファイル生成等の補助用途のみ |

> **方針**: Rust をコアとし、全処理を単一バイナリで完結させる。Node.js / npm に依存しない。Deno は補助用途に限定。

---

## 2. 機能要件

### 2.1 起動・セットアップ

#### 要件

- `--root` 引数でワークスペースルートを指定する
- 起動時にワークスペース内の言語を自動判別する
- tree-sitter パーサーは Cargo クレートとしてビルド時に静的にバイナリへ組み込まれる（実行時ダウンロード不要）
- 起動時にバックグラウンドでバージョンチェックを行い、結果をログ出力する（非ブロッキング）

#### バージョンチェック仕様

- GitHub API（`GET /repos/tonrakun/T0K3N-MCP/releases/latest`）から最新リリースの `tag_name` を取得する
- 実行中バージョンと semver 比較する
- `実行中 < 最新リリース` → `info: ⬆ Update available: vX.X.X → vY.Y.Y` + リリース URL をログ出力
- `実行中 > 最新リリース` → `info: 🧪 Beta Preview: running vX.X.X (latest release: vY.Y.Y)` をログ出力
- `実行中 == 最新リリース` → `debug` レベルのみ（通常ログに出力しない）
- API エラー・タイムアウト（8 秒）→ `debug` レベルのみ、起動をブロックしない

#### 言語判別ロジック

1. ワークスペース直下のファイル拡張子を集計
2. `package.json` / `Cargo.toml` / `go.mod` 等のマニフェストファイルで補完
3. 判別結果をログ出力する（実際のパーサー選択はバイナリ内で静的に決定済み）

---

### 2.2 ファイル読み取り系

以下は Sophon-MCP を参考に Rust で実装する。

| ツール | 変更 |
|---|---|
| `read_directory_tree` | Rust 実装 |
| `read_markdown_toc` | Rust 実装 |
| `read_markdown_section` | Rust 実装 |
| `read_git_diff` | Rust 実装 |
| `search_file` | Rust 実装 |
| `read_json_yaml_keys` | Rust 実装 |
| `read_json_yaml_value` | Rust 実装 |

#### 2.2.1 `read_code_skeleton`（拡張）

tree-sitter AST ベースで実装する。

**入力**

```ts
{
  path: string;           // コードファイルのパス
  include_blocks?: boolean; // if/for等のブロックも含む（デフォルト: false）
}
```

**出力**

```ts
{
  ok: true;
  path: string;
  language: string;       // 検出言語
  symbols: {
    id: string;
    kind: "function" | "class" | "method" | "arrow" | "block";
    name: string;
    signature: string;    // シグネチャのみ（ボディなし）
    start_line: number;
    end_line: number;
  }[];
  token_count: number;
}
```

#### 2.2.2 `read_code_body`

tree-sitter 移行後も `read_code_skeleton` が返す `id` で本文取得できるよう互換性を維持する。

#### 2.2.3 `semantic_search`

「スケルトン取得 → Claude サブプロセスで関連 ID 特定 → 本文返却」方式で実装する。

**実装方針詳細**

| 項目 | 仕様 |
|---|---|
| サブプロセス | `claude -p "<プロンプト>"` を `std::process::Command` で起動 |
| 認証 | Claude Code CLI が管理（T0K3N-MCP は API キーを一切保持しない） |
| `claude` 未インストール時 | `claude コマンドが見つかりません` エラーを返し処理を中断 |
| `--root` との関係 | サブプロセスには `--root` パスを渡さない。スケルトンテキストのみをプロンプトに埋め込む |
| ログ出力 | サブプロセスの stdout のみ取得し、認証情報は一切扱わない |

> **前提条件**: 実行環境に Claude Code CLI（`claude` コマンド）がインストール・認証済みであること。

---

### 2.3 Web 取得系

#### 2.3.1 `fetch_webpage`（変更）

HTML を取得し、MD に変換して TOC を返す。**ボディは返さない**。

**変更点**

- `htmlToMarkdown`: `htmd` クレート（Rust ネイティブ）を使用
- レスポンス: 全文 MD → **TOC のみ**（`read_markdown_toc` と同じ構造）
- 取得した MD はメモリキャッシュ（セッション中有効）

**入力**

```ts
{
  url: string;
  compress?: boolean; // デフォルト: true
}
```

**出力**

```ts
{
  ok: true;
  url: string;
  toc: { level: number; text: string; anchor: string }[];
  token_count: number;        // 全文の推定トークン数
  cached: boolean;
}
```

#### 2.3.2 `read_webpage_section`（新規）

`fetch_webpage` でキャッシュされた MD から指定セクションを返す。

**入力**

```ts
{
  url: string;
  anchors: string[];  // fetch_webpage の toc[].anchor を指定
}
```

**出力**

```ts
{
  ok: true;
  url: string;
  sections: {
    anchor: string;
    content: string;
    token_count: number;
  }[];
  total_token_count: number;
}
```

**フロー**

```
1. fetch_webpage(url)     → toc を取得（キャッシュに MD 保持）
2. LLM が必要な anchor を選択
3. read_webpage_section(url, anchors) → 該当セクションのみ返却
```

---

### 2.4 ドキュメント変換系（新規）

#### 2.4.1 `convert_document`

PDF / DOCX 等のドキュメントを MD に変換し、TOC を返す。  
変換後は `read_markdown_toc` / `read_markdown_section` と同じフローで読み取る。

**対応フォーマット**

| 形式 | ライブラリ |
|---|---|
| PDF | `pdf-extract` |
| DOCX | `docx-rs` |
| その他 | エラー返却（将来拡張） |

**入力**

```ts
{
  path: string;  // ワークスペース内のドキュメントパス
}
```

**出力**

```ts
{
  ok: true;
  path: string;
  format: "pdf" | "docx";
  toc: { level: number; text: string; anchor: string }[];
  token_count: number;     // 全文の推定トークン数
  tmp_path: string;        // 変換後 MD の一時ファイルパス（read_markdown_section に渡す）
}
```

> 変換後の MD は `/tmp/t0k3n-<hash>.md` に書き出す。`read_markdown_section(path: tmp_path, anchors)` でそのまま使用可能。一時ファイルはセッション終了時に自動クリーンアップする。

---

### 2.5 テキスト圧縮系

| ツール | 変更 |
|---|---|
| `compress_text` | Rust 実装 |

---

### 2.6 コンテキスト管理系

#### ツール

| ツール | 変更 |
|---|---|
| `count_tokens` | Rust 実装 |
| `summarize_conversation` | Rust 実装 |

#### 2.6.1 `check_budget`（新規）

トークンバジェットを管理し、残量に応じた読み取り戦略を返す。  
`used` はサーバーがセッション中の全ツールレスポンスの `token_count` を自動集計する（LLM の自己申告不要）。

**入力**

```ts
{
  budget: number;          // 使用可能な最大トークン数
  candidates: {
    description: string;   // 例: "src/auth.ts のスケルトン"
    estimated_tokens: number;
  }[];
}
```

**出力**

```ts
{
  budget: number;
  used: number;            // サーバー自動集計（セッション中の累計 token_count）
  remaining: number;
  recommended: {
    description: string;
    estimated_tokens: number;
    fits: boolean;
  }[];
  strategy: "full" | "skeleton_only" | "toc_only" | "skip";
  // full        : 残量十分、全文読める
  // skeleton_only: スケルトン/TOC のみ推奨
  // toc_only    : TOC のみ推奨
  // skip        : 残量不足
}
```

---

### 2.7 記憶系

| ツール | 変更 |
|---|---|
| `memory_save` | Rust 実装 |
| `memory_get` | Rust 実装 |
| `memory_list` | Rust 実装 |
| `memory_delete` | Rust 実装 |

---

### 2.8 タスク系

| ツール | 変更 |
|---|---|
| `task_create` | Rust 実装 |
| `task_update` | Rust 実装 |
| `task_get` | Rust 実装 |
| `task_list` | Rust 実装 |
| `task_delete` | Rust 実装 |

---

### 2.9 セッション系

| ツール | 変更 |
|---|---|
| `session_snapshot` | Rust 実装 |
| `session_restore` | Rust 実装 |
| `session_list` | Rust 実装 |

---

### 2.10 依存関係系（新規）

#### 2.10.1 `read_code_deps`

コードファイルの import/export 依存グラフを返す。「このファイルは何を使っているか」「このファイルは何から使われているか」をフルコンテンツなしに把握できる。

**対応言語**

| 言語 | import 抽出方法 | imported_by 検索 |
|---|---|---|
| Rust | `use` 文（regex） | `use` / `mod` 含む行を全ファイル検索 |
| Python | `import` / `from ... import`（regex） | `import` / `from` 含む行を検索 |
| JavaScript / TypeScript | ES import / `require()`（regex）・相対パス解決 | `import` / `require` 含む行を検索 |
| Go | `import` ブロック（regex） | `"` 含む行を検索 |

**入力**

```ts
{
  path: string;
  direction?: "imports" | "imported_by" | "both"; // デフォルト: "both"
}
```

**出力**

```ts
{
  path: string;
  language: string;
  imports: {
    raw: string;          // 生 import 文字列
    resolved?: string;    // 相対パス → 実ファイルパスに解決（JS/TS のみ）
    symbols: string[];    // インポートシンボル一覧
  }[];
  imported_by: string[];  // このファイルを参照するワークスペース内ファイルパス一覧（最大 200 件）
  token_count: number;
}
```

---

### 2.11 汎用アウトライン系（新規）

#### 2.11.1 `read_file_outline`

ファイル種別を自動判別し、適切なスケルトン / TOC / キー構造を返す統合エントリーポイント。LLM がツールを選択するコストを削減する。

**種別判別ロジック**

| 拡張子 | kind | 内部呼び出し |
|---|---|---|
| `.rs` `.py` `.js` `.ts` `.go` 等 | `"code"` | `read_code_skeleton` |
| `.md` `.markdown` `.mdx` | `"markdown"` | `read_markdown_toc` |
| `.json` `.jsonc` | `"json"` | `read_json_yaml_keys` |
| `.yaml` `.yml` | `"yaml"` | `read_json_yaml_keys` |
| その他 | `"unknown"` | — |

**入力**

```ts
{
  path: string;  // ワークスペース内の任意ファイルパス
}
```

**出力**

```ts
{
  path: string;
  kind: "code" | "markdown" | "json" | "yaml" | "unknown";
  language?: string;         // コードファイルの場合のみ（例: "rust", "typescript"）
  outline: SkeletonItem[] | TocEntry[] | string[] | null;
  token_count: number;
}
```

---

### 2.13 Git 拡張系

#### 2.13.1 `read_git_log`

コミット履歴を構造化して返す。`read_git_diff` との対になるツール。

**入力**

```ts
{
  path?: string;    // 対象ファイル/ディレクトリ（省略時: 全コミット）
  author?: string;  // 著者名またはメールのサブ文字列
  since?: string;   // 例: "2024-01-01" / "2 weeks ago"
  until?: string;
  limit?: number;   // デフォルト: 20、最大: 100
}
```

**出力**

```ts
{
  entries: {
    sha: string;
    sha_short: string;
    author: string;
    date: string;        // YYYY-MM-DD
    message: string;
    files: string[];
  }[];
  token_count: number;
}
```

#### 2.13.2 `read_git_blame_body`

`read_code_skeleton` が返す `start_line` / `end_line` を使い、関数単位の blame を取得する。

**入力**

```ts
{
  path: string;
  start_line: number;  // read_code_skeleton の start_line をそのまま使用
  end_line: number;
}
```

**出力**

```ts
{
  path: string;
  lines: {
    line_no: number;
    sha_short: string;
    author: string;
    date: string;
    content: string;
  }[];
  token_count: number;
}
```

---

### 2.14 シンボル検索系

#### 2.14.1 `read_symbol_usages`

ワークスペース全体でシンボル名の使用箇所を検索する。`search_file`（単一ファイル）の全ワークスペース版。正規表現のワードバウンダリでマッチし、コードファイル（rs/py/js/ts/go/cpp/java/rb 等）のみを対象とする。

**入力**

```ts
{
  symbol: string;   // 検索するシンボル名
  path?: string;    // 検索対象をこのファイル/ディレクトリに絞る（省略時: 全ワークスペース）
}
```

**出力**

```ts
{
  symbol: string;
  usages: {
    path: string;
    line: number;
    content: string;
    context: string[];  // 前後1行
  }[];
  total: number;
  truncated: boolean;   // 100件上限に達した場合 true
  token_count: number;
}
```

---

### 2.15 OpenAPI 系

#### 2.15.1 `read_openapi`

OpenAPI / Swagger (JSON または YAML) をパースし、エンドポイント一覧をコンパクトに返す。大きなスペックファイルを全文読み込む代わりに使う。

**対応フォーマット**: OpenAPI 3.x / Swagger 2.0（JSON・YAML）

**入力**

```ts
{
  path: string;  // ワークスペース内の OpenAPI ファイルパス
}
```

**出力**

```ts
{
  title?: string;
  version?: string;
  base_url?: string;
  spec_version: string;
  endpoints: {
    method: string;
    path: string;
    operation_id?: string;
    summary?: string;
    tags: string[];
    parameters: string[];    // "name (in, required?)" 形式
    request_body?: string;   // content-type
    responses: string[];     // "200 OK" 形式
  }[];
  token_count: number;
}
```

---

### 2.16 環境変数スキーマ系

#### 2.16.1 `read_env_schema`

`.env.example` / `.env.sample` / `.env.template` / `docker-compose.yml` から環境変数の定義一覧を抽出する。コメントを description として取り込む。

**対応ファイル**

| ファイル | 抽出内容 |
|---|---|
| `.env.example` / `.env.sample` / `.env.template` / `.env.dist` | コメント → description、`KEY=value` → key + default |
| `docker-compose.yml` | `environment:` ブロック（リスト・マップ両形式） |

**入力**

```ts
{
  path?: string;  // 省略時: ワークスペースルートを自動スキャン
}
```

**出力**

```ts
{
  vars: {
    key: string;
    default_value?: string;
    description?: string;
    required: boolean;
    source: string;  // ファイル名
  }[];
  sources: string[];  // スキャンしたファイル一覧
  token_count: number;
}
```

---

### 2.12 デバッグ系

#### 2.12.1 `debug_info`

サーバー診断情報を返す。バージョン・root パス・DB 状態・登録ツール一覧を確認できる。

**出力**

```ts
{
  ok: true;
  version: string;
  root: string;
  db_status: "ok" | string;
  tool_count: number;
  tools: string[];
  timestamp_unix: number;
}
```

---

## 3. 非機能要件

### 3.1 パフォーマンス

- MCP ツール応答: 通常ファイルで **200ms 以内**（tree-sitter パース含む）
- Web フェッチ: タイムアウト 15 秒

### 3.2 互換性

- 単一バイナリ配布（Node.js / npm 不要）
- MCP プロトコル準拠（`rmcp` クレート）
- Linux / macOS / Windows 対応

### 3.3 セキュリティ

- `--root` 外へのパス解決禁止
- シンボリックリンクの root 外追跡禁止
- `fetch_webpage` / `read_webpage_section` は外部 URL のみ（root 制約適用外）

### 3.4 データ保存

```
.t0k3n/
  t0k3n.db       ← SQLite（記憶・タスク・セッション）
```

---

## 4. ツール一覧サマリ

### ファイル読み取り系

| ツール | 種別 | 説明 |
|---|---|---|
| `read_directory_tree` | Rust 実装 | .gitignore 適用済みディレクトリツリー |
| `read_markdown_toc` | Rust 実装 | MD の見出し一覧 |
| `read_markdown_section` | Rust 実装 | anchor 指定でセクション取得 |
| `read_code_skeleton` | Rust 実装 | tree-sitter AST ベーススケルトン（language フィールド・複数行シグネチャ対応） |
| `read_code_body` | Rust 実装 | スケルトン ID 指定で本文取得 |
| `read_file_outline` | Rust 実装 | ファイル種別自動判別の統合アウトライン取得 |
| `read_code_deps` | Rust 実装 | import/imported_by 依存グラフ（Rust/Python/JS/TS/Go） |
| `read_symbol_usages` | Rust 実装 | ワークスペース全体シンボル使用箇所検索 |
| `read_git_diff` | Rust 実装 | 圧縮済み git diff |
| `read_git_log` | Rust 実装 | 構造化コミットログ（著者・日付・変更ファイル） |
| `read_git_blame_body` | Rust 実装 | 関数単位の行 blame（著者・日付） |
| `search_file` | Rust 実装 | キーワードマッチ＋文脈 |
| `semantic_search` | Rust 実装 | 意味検索 |
| `read_json_yaml_keys` | Rust 実装 | JSON/YAML キー構造 |
| `read_json_yaml_value` | Rust 実装 | キーパス指定で値取得 |

### Web 取得系

| ツール | 種別 | 説明 |
|---|---|---|
| `fetch_webpage` | Rust 実装 | HTML→MD変換（htmd）・TOC返却 |
| `read_webpage_section` | Rust 実装 | anchor 指定でセクション取得 |

### ドキュメント変換系

| ツール | 種別 | 説明 |
|---|---|---|
| `convert_document` | Rust 実装 | PDF/DOCX → MD 変換・TOC返却 |

### テキスト圧縮系

| ツール | 種別 | 説明 |
|---|---|---|
| `compress_text` | Rust 実装 | ノイズ除去・圧縮 |

### コンテキスト管理系

| ツール | 種別 | 説明 |
|---|---|---|
| `count_tokens` | Rust 実装 | トークン数計測 |
| `summarize_conversation` | Rust 実装 | 会話履歴要約 |
| `check_budget` | Rust 実装 | トークンバジェット管理・戦略返却 |

### 記憶系

| ツール | 種別 | 説明 |
|---|---|---|
| `memory_save` | Rust 実装 | キー・バリュー永続保存 |
| `memory_get` | Rust 実装 | キー指定で取得 |
| `memory_list` | Rust 実装 | タグ・キーワードで一覧 |
| `memory_delete` | Rust 実装 | キー指定で削除 |

### タスク系

| ツール | 種別 | 説明 |
|---|---|---|
| `task_create` | Rust 実装 | タスク作成 |
| `task_update` | Rust 実装 | タスク更新 |
| `task_get` | Rust 実装 | タスク取得 |
| `task_list` | Rust 実装 | タスク一覧 |
| `task_delete` | Rust 実装 | タスク削除 |

### セッション系

| ツール | 種別 | 説明 |
|---|---|---|
| `session_snapshot` | Rust 実装 | 作業状態スナップショット |
| `session_restore` | Rust 実装 | スナップショット復元 |
| `session_list` | Rust 実装 | スナップショット一覧 |

### OpenAPI 系

| ツール | 種別 | 説明 |
|---|---|---|
| `read_openapi` | Rust 実装 | OpenAPI/Swagger エンドポイント一覧取得 |

### 環境変数スキーマ系

| ツール | 種別 | 説明 |
|---|---|---|
| `read_env_schema` | Rust 実装 | .env.example / docker-compose.yml から変数定義抽出 |

### デバッグ系

| ツール | 種別 | 説明 |
|---|---|---|
| `debug_info` | Rust 実装 | サーバー診断情報（バージョン・DB・登録ツール一覧） |

---

## 5. 実装フェーズ

### Phase 1 — Rust MVP

- [x] Cargo プロジェクト初期化・`rmcp` セットアップ
- [x] 起動時言語判別（ファイル拡張子・マニフェストファイルベース）
- [x] ファイル読み取り系ツール全実装
- [x] `read_code_skeleton` / `read_code_body`（regex ベース実装、tree-sitter は Phase 2）
- [x] `fetch_webpage`（htmd）+ `read_webpage_section`
- [x] `convert_document`（PDF/DOCX → MD・一時ファイル）
- [x] `compress_text`
- [x] `count_tokens` / `check_budget`
- [x] SQLite（記憶・タスク・セッション）

### Phase 2 — 安定化・最適化

- [x] ベンチマーク測定
- [x] エラーハンドリング強化（パストラバーサル防止・入力バリデーション）
- [x] MCP Instructions 整備（ツールの使い方を LLM に伝える）
- [x] バイナリ配布（GitHub Actions release.yml）
- [x] `read_git_diff`（圧縮済み git diff・stat_only オプション）
- [x] `semantic_search`（claude CLI サブプロセス方式）
- [x] `read_code_skeleton` に `language` フィールド追加・複数行シグネチャ対応（tree-sitter の `extract_node_signature` 導入）
- [x] `debug_info` ツール（サーバー診断・登録ツール一覧）
- [x] `read_code_deps`（依存関係グラフ・imports / imported_by・Rust/Python/JS/TS/Go 対応）
- [x] `read_file_outline`（ファイル種別自動判別の統合アウトラインエントリーポイント）
- [x] バックグラウンド自動バージョンチェック（GitHub Releases API・Beta Preview 判定）
- [x] `read_git_log`（構造化コミットログ・author/since/until/path フィルタ）
- [x] `read_git_blame_body`（関数単位の行 blame・porcelain パース）
- [x] `read_symbol_usages`（ワークスペース全体シンボル使用箇所検索・最大 100 件）
- [x] `read_openapi`（OpenAPI 3.x / Swagger 2.0 エンドポイント一覧）
- [x] `read_env_schema`（.env.example / docker-compose.yml 環境変数スキーマ抽出）

### Phase 3 — 拡張（要検討）

- [ ] Deno スクリプト連携（補助用途）
- [ ] 新言語対応（Cargo クレート追加・新リリースで提供・GitHub Issues でリクエスト可）
- [ ] 対応フォーマット追加（CSV / TOML 等）

---

## 6. 決定事項

| # | 内容 | 決定 |
|---|---|---|
| 1 | パッケージ名・バイナリ名 | **`t0k3n-mcp`** |
| 2 | tree-sitter パーサーの追加方式 | **Cargo クレートとしてビルド時に静的バンドル**（実行時 DL なし）。新言語対応は新リリースで提供 |