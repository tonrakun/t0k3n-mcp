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

> **方針**: Rust をコアとし、全処理を単一バイナリで完結させる。Node.js / npm に依存しない。

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

### 2.17 `read_call_graph` クロスファイル拡張

既存の `read_call_graph`（単一ファイル内 callers/callees）に `depth` パラメータを追加し、ファイルをまたいだ呼び出しグラフをトレースできるようにする。後方互換を維持し、`depth: 0`（デフォルト）で従来動作を保つ。

**変更点（後方互換）**

```ts
{
  path: string;
  function_name: string;
  direction?: "callers" | "callees" | "both"; // デフォルト: "both"
  depth?: number; // 0=単一ファイル内のみ（現行動作）、1以上=クロスファイルトレース（デフォルト: 0、最大: 5）
}
```

**depth >= 1 の挙動**

- callees: 呼び出し先関数が外部ファイルにある場合、そのファイルを解析して再帰トレース
- callers: `read_symbol_usages` と同じ手法でワークスペース全体を検索してから解析
- 循環参照は自動検出してスキップ（visited set 管理）

---

### 2.18 パッケージマニフェスト系（新規）

#### 2.18.1 `read_package_manifest`

`package.json` / `Cargo.toml` / `pyproject.toml` / `go.mod` / `pom.xml` / `build.gradle` を統一フォーマットで返す。`read_json_yaml_value` で個別キーを取得するより効率的で、複数ファイルを横断した依存関係の概観を得られる。

**対応ファイル**

| ファイル | エコシステム |
|---|---|
| `package.json` | Node.js / npm / yarn / pnpm |
| `Cargo.toml` | Rust / Cargo |
| `pyproject.toml` / `requirements.txt` | Python |
| `go.mod` | Go |
| `build.gradle` / `build.gradle.kts` | Java / Kotlin (Gradle) |
| `pom.xml` | Java (Maven) |

**入力**

```ts
{
  path?: string; // 省略時: ワークスペースルートを自動スキャン
}
```

**出力**

```ts
{
  manifests: {
    path: string;
    ecosystem: string;        // "npm" | "cargo" | "python" | "go" | "gradle" | "maven"
    name?: string;
    version?: string;
    dependencies: {
      name: string;
      version: string;
      kind: "runtime" | "dev" | "build" | "optional";
    }[];
    scripts?: { [name: string]: string }; // npm scripts / Cargo aliases 等
  }[];
  token_count: number;
}
```

---

### 2.19 CI パイプライン系（新規）

#### 2.19.1 `read_ci_pipeline`

GitHub Actions / GitLab CI / CircleCI の YAML をパースし、ワークフロー構造をコンパクトに返す。大きな CI ファイルを全文読む代わりに使う。

**対応フォーマット**

| 形式 | 検出方法 |
|---|---|
| GitHub Actions | `.github/workflows/*.yml` |
| GitLab CI | `.gitlab-ci.yml` |
| CircleCI | `.circleci/config.yml` |

**入力**

```ts
{
  path?: string; // 省略時: ワークスペースルートを自動スキャン
}
```

**出力**

```ts
{
  pipelines: {
    path: string;
    format: "github-actions" | "gitlab-ci" | "circleci";
    workflows: {
      name: string;
      triggers: string[];       // push / pull_request / schedule 等
      jobs: {
        name: string;
        runs_on?: string;       // ubuntu-latest 等
        needs?: string[];       // 依存ジョブ
        steps: string[];        // step name / uses の一覧
        env_vars: string[];     // 参照している env var 名
      }[];
    }[];
  }[];
  token_count: number;
}
```

---

### 2.20 バッチ読み取り系（新規）

#### 2.20.1 `batch_read`

複数の読み取り操作を 1 回のツール呼び出しで並列実行する。MCP のラウンドトリップを削減し、スケルトン → 本文の一括取得フローを高速化する。

**対応操作**

| operation | 相当ツール |
|---|---|
| `code_skeleton` | `read_code_skeleton` |
| `code_body` | `read_code_body` |
| `markdown_section` | `read_markdown_section` |
| `json_value` | `read_json_yaml_value` |
| `file_outline` | `read_file_outline` |

**入力**

```ts
{
  reads: {
    id: string;               // レスポンス内で結果を識別するためのクライアント指定 ID
    operation: "code_skeleton" | "code_body" | "markdown_section" | "json_value" | "file_outline";
    path: string;
    // operation 別オプション（各ツールの入力と同じ）
    ids?: string[];           // code_body 用
    anchors?: string[];       // markdown_section 用
    key_path?: string;        // json_value 用
    include_blocks?: boolean; // code_skeleton 用
  }[];
}
```

**出力**

```ts
{
  results: {
    id: string;
    ok: boolean;
    data: any;                // 各ツールのレスポンスと同じ構造
    error?: string;
    token_count: number;
  }[];
  total_token_count: number;
}
```

---

### 2.21 ワークスペース統計系（新規）

#### 2.21.1 `read_workspace_stats`

コードベース全体の統計サマリを返す。`read_token_map`（ファイル単位のトークン数一覧）より高レベルな概観で、初回調査時の把握コストを削減する。

**入力**

```ts
{
  glob?: string; // フィルタ（例: "src/**/*.ts"）
}
```

**出力**

```ts
{
  total_files: number;
  total_lines: number;
  total_tokens: number;
  by_language: {
    language: string;
    files: number;
    lines: number;
    tokens: number;
    pct: number;              // 全体に占める割合（%）
  }[];
  largest_files: {            // トークン数 Top 10
    path: string;
    tokens: number;
  }[];
  token_count: number;        // このレスポンス自体のトークン数
}
```

---

### 2.22 Git スタッシュ系（新規）

#### 2.22.1 `read_git_stash`

スタッシュ一覧と各エントリの diff を token-efficient に返す。

**入力**

```ts
{
  index?: number;      // 省略時: 一覧のみ返す。指定時: そのエントリの diff も返す
  stat_only?: boolean; // true=差分統計のみ（デフォルト: false）
}
```

**出力**

```ts
{
  stashes: {
    index: number;
    name: string;    // "stash@{0}" 形式
    message: string;
    date: string;
    branch: string;
  }[];
  diff?: string;     // index 指定時のみ
  token_count: number;
}
```

---

### 2.23 インターフェース適合系（新規）

#### 2.23.1 `read_interface_conformance`

指定した interface / trait / abstract class を実装・継承しているコンクリート型をワークスペース全体から検索する。大規模コードベースでのリファクタリング影響範囲の把握に使う。

**対応言語**

| 言語 | 対象構文 |
|---|---|
| TypeScript | `implements InterfaceName` |
| Go | 構造的型付けのため `read_symbol_usages` ベースで推定 |
| Rust | `impl TraitName for TypeName` |
| Java / Kotlin | `implements` / `extends` |

**入力**

```ts
{
  name: string;    // interface / trait / abstract class 名
  path?: string;  // 検索スコープ（省略時: 全ワークスペース）
}
```

**出力**

```ts
{
  name: string;
  kind: "interface" | "trait" | "abstract_class";
  implementations: {
    type_name: string;
    path: string;
    line: number;
    language: string;
  }[];
  total: number;
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
| `search_file` | Rust 実装 | キーワードマッチ＋文脈 |
| `semantic_search` | Rust 実装 | 意味検索 |
| `read_json_yaml_keys` | Rust 実装 | JSON/YAML/TOML キー構造 |
| `read_json_yaml_value` | Rust 実装 | キーパス指定で値取得（JSON/YAML/TOML） |
| `read_type_skeleton` | Rust 実装 | 型定義スケルトン（TS interface/type/enum・Go struct/interface・Rust struct/enum/trait） |
| `read_call_graph` | Rust 実装 | 関数の呼び出し先・呼び出し元グラフ（単一ファイル内 / depth 指定でクロスファイル対応） |
| `read_token_map` | Rust 実装 | ワークスペース内ファイルのトークン数マップ（glob フィルタ・降順ソート） |
| `read_workspace_stats` | Rust 実装 | コードベース全体の言語別統計サマリ（ファイル数・行数・トークン数） |
| `read_interface_conformance` | Rust 実装 | interface / trait 実装型の検索（TS/Go/Rust/Java/Kotlin） |
| `batch_read` | Rust 実装 | 複数の読み取り操作を 1 コールで並列実行（ラウンドトリップ削減） |

### Git 拡張系

| ツール | 種別 | 説明 |
|---|---|---|
| `read_git_diff` | Rust 実装 | 圧縮済み git diff |
| `read_git_log` | Rust 実装 | 構造化コミットログ（著者・日付・変更ファイル） |
| `read_git_blame_body` | Rust 実装 | 関数単位の行 blame（著者・日付） |
| `read_changed_files` | Rust 実装 | ブランチ間の変更ファイル一覧（ステータス・追加/削除行数） |
| `read_git_stash` | Rust 実装 | スタッシュ一覧と diff 取得 |

### DB スキーマ系

| ツール | 種別 | 説明 |
|---|---|---|
| `read_db_schema` | Rust 実装 | Prisma / SQL スキーマのテーブル/モデル一覧（自動検出対応） |
| `read_db_table` | Rust 実装 | テーブル/モデルのフィールド定義詳細取得 |

### CSS 系

| ツール | 種別 | 説明 |
|---|---|---|
| `read_css_skeleton` | Rust 実装 | CSS/SCSS セレクタ一覧（プロパティ数・行範囲） |
| `read_css_body` | Rust 実装 | セレクタ ID 指定でルールセット本文取得 |

### GraphQL 系

| ツール | 種別 | 説明 |
|---|---|---|
| `read_graphql_schema` | Rust 実装 | GraphQL スキーマの型一覧（type/input/enum/interface） |
| `read_graphql_type` | Rust 実装 | 型名指定でフィールド定義詳細取得 |

### テスト系

| ツール | 種別 | 説明 |
|---|---|---|
| `read_test_skeleton` | Rust 実装 | テストファイルのスイート/テスト一覧（Jest/pytest/Cargo/Go/JUnit/RSpec） |
| `read_test_results` | Rust 実装 | テスト結果テキストのパース・サマリ返却（フレームワーク自動検出） |

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

### パッケージマニフェスト系

| ツール | 種別 | 説明 |
|---|---|---|
| `read_package_manifest` | Rust 実装 | package.json / Cargo.toml / go.mod 等を統一フォーマットで返す |

### CI パイプライン系

| ツール | 種別 | 説明 |
|---|---|---|
| `read_ci_pipeline` | Rust 実装 | GitHub Actions / GitLab CI / CircleCI ワークフロー構造取得 |

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

- [x] 新言語対応（C# / PHP — v2.0.0 で追加）
- [x] 対応フォーマット追加（TOML — `read_json_yaml_keys` / `read_json_yaml_value` / `read_file_outline` で対応）
- [x] `read_type_skeleton`（TS/Go/Rust 型定義スケルトン）
- [x] `read_call_graph`（関数呼び出しグラフ・単一ファイル内 callers/callees）
- [x] `read_token_map`（ワークスペーストークンマップ・glob フィルタ）
- [x] `read_changed_files`（ブランチ間変更ファイル一覧・ステータス・diff 行数）
- [x] `read_db_schema` / `read_db_table`（Prisma / SQL スキーマ段階的読み取り）
- [x] `read_css_skeleton` / `read_css_body`（CSS セレクタ段階的読み取り）
- [x] `read_graphql_schema` / `read_graphql_type`（GraphQL スキーマ段階的読み取り）
- [x] `read_test_skeleton`（テストスイート構造取得・6フレームワーク対応）
- [x] `read_test_results`（テスト結果パース・フレームワーク自動検出）
- [x] `read_proto_schema` / `read_proto_type`（Protocol Buffers スキーマ段階的読み取り）
- [x] `read_notebook_cells` / `read_notebook_cell`（Jupyter ノートブック段階的読み取り）
- [x] `read_log_tail`（ログファイル末尾取得・レベル/パターンフィルタ）
- [x] `read_stack_trace`（スタックトレース→ソースコンテキスト自動解決）

### Phase 4 — 拡張 v2.3+

- [x] 新言語対応（Java / Kotlin — `read_code_skeleton` / `read_type_skeleton` / `read_call_graph`）
- [x] 新言語対応（Swift — iOS 開発向け）
- [x] 新言語対応（Ruby — Rails 向け）
- [x] 新言語対応（Lua — ゲーム・組み込みスクリプト向け）
- [x] `read_call_graph` クロスファイル対応（`depth` パラメータ追加・循環参照検出）
- [x] `read_package_manifest`（package.json / Cargo.toml / pyproject.toml / go.mod / pom.xml / build.gradle 統一フォーマット）
- [x] `read_ci_pipeline`（GitHub Actions / GitLab CI / CircleCI ワークフロー構造取得）
- [x] `batch_read`（複数読み取り操作の 1 コール並列実行）
- [x] `read_workspace_stats`（コードベース全体の言語別統計サマリ）
- [x] `read_git_stash`（スタッシュ一覧と diff 取得）
- [x] `read_interface_conformance`（interface / trait 実装型の全ワークスペース検索）
- [ ] ダッシュボード強化（ツール使用統計・累計トークン節約量の可視化）

---

## 6. 決定事項

| # | 内容 | 決定 |
|---|---|---|
| 1 | パッケージ名・バイナリ名 | **`t0k3n-mcp`** |
| 2 | tree-sitter パーサーの追加方式 | **Cargo クレートとしてビルド時に静的バンドル**（実行時 DL なし）。新言語対応は新リリースで提供 |