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

- GitHub API（`GET /repos/tonrakun/t0k3n-mcp/releases/latest`）から最新リリースの `tag_name` を取得する
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

#### 2.2.4 `read_code_sketch`（新規）

`read_code_skeleton`（シグネチャのみ）と `read_code_body`（全文）の中間ズームレベル（ズーム 1.5）。`read_code_body` と同じ skeleton ID（`kind:start-end`）を受け取り、各シンボルの **制御フロー骨格** を返す。

- 残す行: シグネチャ、分岐・ループ（`if`/`else`/`for`/`while`/`match`/`switch`/`case`/`try`/`catch`/`return`/… の語境界マッチ）、ブロック開閉・区切り（`{` 終端・`:` 終端・`=>`・`}`・`end` 等）、関数/メソッド呼び出しを含む行
- 畳む行: 純データ行（単純代入・リテラル・構造体/配列初期化・コメント専用行）の連続を、最初の行のインデントを保った 1 本の `… N lines …` プレースホルダに置換（言語別コメントトークン: `//` / `#` / `--`）
- body 比 60〜70% 削減を見込む。実装は tree-sitter ではなく行ベースのヒューリスティックで全言語横断・純関数化しユニットテスト

**入力**

```ts
{
  path: string;     // root 相対パス
  ids: string[];    // read_code_skeleton が返す ID（例: 'function:10-25'）
}
```

**出力**

```ts
{
  items: Array<{
    id: string;
    sketch: string;          // 制御フロー骨格（畳んだ箇所は '… N lines …'）
    original_lines: number;  // 元の本文行数
    sketch_lines: number;    // スケッチ後の行数
  }>;
  token_count: number;
}
```

**フロー**: `read_code_skeleton` → `read_code_sketch(ids)` で「何をしているか」を把握 → 本当に必要なシンボルだけ `read_code_body` で全文取得。

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
  factor?: boolean;           // 類似結果をテンプレート + 差分に因数分解（デフォルト false）
}
```

**出力**

```ts
{
  results: {
    id: string;
    ok: boolean;
    data: any;                // 各ツールのレスポンスと同じ構造（因数分解時は {template_ref, diff}）
    error?: string;
    template_ref?: string;    // 因数分解された場合、差分の基準となったテンプレートの ID
    token_count: number;
  }[];
  factored: number;           // 因数分解された結果の件数
  total_token_count: number;
}
```

**テンプレート因数分解（`factor: true`）**

マイグレーション・テスト fixture など互いに酷似した複数ファイルを読む際、各類似グループの先頭 1 件を正規形（テンプレート）として全文を残し、残りはテンプレートとの unified diff（`{template_ref, diff}`）に置換してトークンを削減する。

- 類似度判定: `similar` クレートの行ベース ratio。閾値 0.5 以上のペアを同一グループに集約
- 採用条件: diff が候補本文より小さい場合のみ（小ファイルでヘッダ overhead が勝つ場合は全文のまま）
- 比較テキスト抽出: 配列要素の `content` / `section` / `value` を行連結、スカラー文字列はそのまま、その他は pretty JSON にフォールバック

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

### 2.24 コマンド実行系（新規）

#### 2.24.1 `run_command`

シェルコマンドを実行し、トークン効率の良い出力を返す。生の全出力を AI に渡す代わりに、成功時は最終サマリ（末尾 ~30 行）、失敗時はエラー行・警告行・末尾 ~20 行のみを返す。

**対象コマンドカテゴリ**

| カテゴリ | 代表コマンド |
|---|---|
| ビルドツール | `cargo build`, `go build`, `make`, `cmake`, `mvn`, `gradle`, `tsc` |
| パッケージマネージャー | `npm install`, `yarn`, `pnpm`, `pip install`, `cargo add` |
| テストランナー | `cargo test`, `pytest`, `jest`, `vitest`, `go test`, `mocha` |
| リンター / フォーマッター | `cargo clippy`, `eslint`, `flake8`, `pylint`, `rubocop` |
| 型チェッカー | `tsc --noEmit`, `mypy`, `pyright` |
| 汎用コマンド | 任意のシェルコマンド |

**フィルタリング戦略**

| 状態 | 返す内容 |
|---|---|
| 成功（exit code 0）| `summary`: 末尾 ~30 非空行（build summary が含まれる） |
| 失敗（exit code ≠ 0）| `errors`: エラー行＋前後コンテキスト / `summary`: 末尾 ~20 行 |
| 常時 | `warnings`: 警告行（rust warning: / npm warn / deprecated 等） |

**入力**

```ts
{
  command: string;        // 実行するシェルコマンド（sh -c / cmd /C 経由）
  cwd?: string;           // 作業ディレクトリ（root からの相対パス。省略時: root）
  timeout_secs?: number;  // タイムアウト秒数（デフォルト: 120、最大: 600）
}
```

**出力**

```ts
{
  command: string;
  exit_code: number;
  success: boolean;
  duration_ms: number;
  summary: string;        // 成功時: 末尾 ~30 行 / 失敗時: 末尾 ~20 行
  errors: string[];       // 抽出されたエラー行（成功時は空）
  warnings: string[];     // 抽出された警告行（常時）
  token_count: number;
}
```

**対応エラーパターン（自動検出）**

- Rust: `error[E0123]`, `error: `, `could not compile`, `aborting due to`
- TypeScript: `error TS1234`, `Found N error(s)`
- Python: `SyntaxError:`, `ImportError:`, `Traceback (most recent call last):`
- Go: `./foo.go:12:5: undefined: Bar`
- npm: `npm ERR!`
- Make/CMake: `make: ***`, `CMake Error:`
- Maven/Gradle: `BUILD FAILURE`, `[ERROR]`, `COMPILATION ERROR`
- テスト: `FAILED`, `failures:`, `● test name`（Jest）

---

### 2.25 ヘルプ・ディスカバリ系（新規）

#### 2.25.1 `help`

**目的**: ツール増加に伴う instructions 肥大化を防ぐ。AI がどのツールを使うべきか不明な時に動的に探索できるようにする。

**引数**:

| パラメータ | 型 | 説明 |
|---|---|---|
| `category` | `string?` | カテゴリ名（file/git/schema/web/notebook/test/log/text/memory/task/session/analysis/cmd/debug）。省略時はカテゴリ名一覧のみ。`"all"` で全カタログ |

**返値**:
- 省略時: カテゴリ名の配列 + 使い方ヒント（最小トークン）
- カテゴリ指定時: そのカテゴリのツール一覧（`name` + `description`）
- `"all"`: 全カテゴリ → ツール一覧のマップ

**設計方針**:
- instructions からツール列挙を排除し「不明なときは `help` を呼ぶ」1 行に置き換える
- ツール追加時は `help` の静的テーブルのみ更新すればよく、instructions は変更不要
- カテゴリ不明時は利用可能カテゴリ一覧をエラーメッセージに含めて返す
- instructions はツール列挙ではなく原則駆動（フル読み禁止 / skeleton→body 2段階読み / check_budget / batch_read / help 案内）。DELTA READS の挙動説明は help で発見できないため instructions に維持する

---

### 2.26 LSP / 型診断系（新規）

#### 2.26.1 `read_type_diagnostics`

静的型診断（LSP 相当）を、言語サーバーを常駐させずに取得する補助ツール。長命の Language Server プロトコルを話す代わりに、各言語が持つ診断エンジン（`rust-analyzer` / `tsserver` / `pyright` / `gopls` が内部でラップしているのと同じコンパイラ・型チェッカー）を **check-only モード** で駆動し、結果を 1 本の重複排除済み・トークン圧縮済み診断リストにまとめて返す。

`run_command` で型チェッカーを生実行すると、コンパイラの冗長な出力（進捗・候補表示・`^^^^` キャレット・"For more information…" トレーラ）がそのまま AI に渡る。本ツールは構造化フィールド `{file, line, col, severity, code, message}` のみを返し、同等の診断を桁違いに少ないトークンで提供する。

**設計方針**

- **オプトイン（デフォルト無効）**。コンパイラ/型チェッカーの起動は重量級のため、`--enable-diagnostics`（または環境変数 `T0K3N_ENABLE_DIAGNOSTICS=1`）で起動したときのみツールを登録する。無効時はツール一覧にも現れず呼び出し不可（`ToolRouter` から経路を除去）。`debug_info` に `diagnostics_enabled` を出力
- 言語サーバー常駐ではなく **ワンショットの check-only 実行**。状態を持たず、どのセッションからでも安全に投機的に呼べる
- チェッカー未導入時は **エラーにせず** `checker_available: false` とインストールヒント（`note`）を返す。編集直後に「型エラーがあれば拾う、なければ静かに通る」用途で気軽に呼べる
- パーサは純関数として分離し、各言語の実出力サンプルでユニットテスト

**対応言語とチェッカー**

| 言語 | 駆動コマンド | 出力形式 |
|---|---|---|
| Rust | `cargo check --message-format=json --quiet --all-targets` | 行区切り JSON（`reason == "compiler-message"` を抽出・primary span を採用） |
| TypeScript / JavaScript | `npx --no-install tsc --noEmit --pretty false` | `path(line,col): error TSxxxx: message` |
| Python | `pyright --outputjson`（未導入時 `mypy --show-column-numbers` にフォールバック） | pyright JSON（0-based → 1-based 補正） / mypy 行 |
| Go | `go vet ./...` | `file:line:col: message` |

**言語判別ロジック**

1. `language` 指定があればそれを使用（rust / typescript / python / go）
2. なければ `path` の拡張子（`.rs` / `.ts,.tsx,.js,…` / `.py,.pyi` / `.go`）
3. それでも不明ならルートのマニフェスト（`Cargo.toml` / `tsconfig.json`・`package.json` / `pyproject.toml`・`setup.py` 等 / `go.mod`）

**入力**

```ts
{
  path?: string;          // 診断対象のファイル/ディレクトリ（root 相対）。省略時はワークスペース全体
  language?: string;      // rust | typescript | python | go（省略時は自動判別）
  severity?: string;      // 最小重要度フィルタ: error | warning | hint（デフォルト: warning）
  max_items?: number;     // 返す診断件数の上限（デフォルト: 100）
  timeout_secs?: number;  // タイムアウト秒数（デフォルト: 180、最大: 600）
}
```

**出力**

```ts
{
  language: string;
  checker: string;            // 実際に使われたチェッカー名（"cargo check" 等）
  checker_available: boolean; // false の場合 diagnostics は空・note にインストールヒント
  note?: string;
  diagnostics: Array<{
    file: string;             // root 相対・スラッシュ区切りに正規化
    line: number;             // 1-based
    col: number;              // 1-based（取得不能時 0）
    severity: "error" | "warning" | "hint";
    code?: string;            // E0308 / TS2322 / reportGeneralTypeIssues 等
    message: string;
  }>;
  summary: { errors: number; warnings: number; hints: number; shown: number; total: number };
  token_count: number;
}
```

**フィルタ・整形**

- `severity` を下限とし、重要度の高い順（error → warning → hint）・file / line / col 順にソート
- 同一 span の重複診断を排除
- `path` 指定時は root 相対プレフィックスで診断を絞り込み（pyright / mypy には対象パスを直接渡す）
- `max_items` 超過分は切り捨て、`summary.total` に全件数・`summary.shown` に表示件数を記録

---

### 2.27 プロジェクトダイジェスト系（新規）

#### 2.27.1 `project_digest`

セッション開始時に毎回繰り返される「ディレクトリツリー → ワークスペース統計 → エントリポイントのスケルトン」探索フェーズを、キャッシュ済みの ~2k トークン要約 1 コールに置換するウォームスタートツール。

- git HEAD ハッシュをキーに `.t0k3n/digest.json` へキャッシュ。HEAD が一致すれば再計算せず `cached: true` で即返却、HEAD 変化時は自動再生成
- `refresh: true` でキャッシュを無視して再生成。`dirty`（未コミット変更あり）を併せて返し、ダイジェストが作業ツリーと乖離しうる旨を通知
- エントリポイント判定: 慣習的ファイル名（`main` / `lib` / `index` / `app` / `server` / `mod` / `__init__` / `cli` / `router` / `config` 等）にスコアを付与し、浅い階層を優先。上位 8 ファイルについて言語・トークン数・上位シンボルシグネチャを収集
- `read_workspace_stats` / `read_directory_tree`（depth 2）/ `read_code_skeleton` を再利用。ディレクトリツリーはバジェット超過時に切り詰め

**入力**

```ts
{
  refresh?: boolean;  // 現在の HEAD のキャッシュがあっても再生成（デフォルト false）
  budget?: number;    // ダイジェストの概算トークンバジェット（デフォルト 2000）
}
```

**出力**

```ts
{
  cached: boolean;
  dirty: boolean;
  digest: {
    git_head: string;          // 短縮ハッシュ（git 非管理時は "no-git"）
    total_files: number;
    total_lines: number;
    total_tokens: number;
    by_language: Array<{ language: string; files: number; lines: number; pct: number }>;
    entry_points: Array<{ path: string; language: string; tokens: number; symbols: string[] }>;
    directory_tree: string;    // 浅い（depth 2）ツリー
  };
  token_count: number;
}
```

---

### 2.28 リネーム系（新規）

#### 2.28.1 `rename_symbol`

シンボル（関数・型・変数等）を全ファイル横断で安全にリネームする書き込み系ツール。エージェントが各使用箇所を手編集する代わりに 1 コールで完結させ、出力トークン（入力の約 5 倍単価）を最小化する。

- `read_symbol_usages` の検出基盤を流用して定義・参照箇所を収集し、識別子境界に一致するものだけ置換（部分一致・コメント/文字列内の誤置換を回避）
- `expected_name` による陳腐化検知（定義シンボルが想定と異なる場合はエラーで候補を返す）・`dry_run`・CRLF / 末尾改行保持は `patch_symbol` の機構を流用
- 出力は影響ファイル数 + 各ファイルの変更行サマリ（行番号 + 置換前後の短い断片）のみ。全文は返さない
- `path` 指定でスコープを単一ファイル/ディレクトリに限定可能

**入力**

```ts
{
  symbol: string;        // 現在のシンボル名
  new_name: string;      // 新しいシンボル名
  path?: string;         // 限定スコープ（省略時はワークスペース全体）
  dry_run?: boolean;     // true で変更せず影響範囲のみ返す（デフォルト false）
}
```

**出力**

```ts
{
  applied: boolean;
  files_changed: number;
  occurrences: number;
  changes: Array<{ path: string; edits: Array<{ line: number; before: string; after: string }> }>;
  token_count: number;
}
```

---

### 2.29 テストカバレッジ系（新規）

#### 2.29.1 `read_test_coverage`

カバレッジレポートを解析し、シンボル単位で「テスト有無・カバー率」を返す。エージェントが「どこを触ると危険か（未カバー領域）」を即座に判断できる。

- 対応フォーマット: `lcov.info` / cobertura XML / coverage.py JSON / `cargo-llvm-cov`（lcov 出力）。ワークスペースを自動スキャンして検出
- 行カバレッジを `read_code_skeleton` のシンボル範囲にマッピングし、シンボルごとの `covered_lines / total_lines / pct` を算出
- `path` 絞り込み・`uncovered_only`（未カバーのみ）・`threshold`（指定率未満のみ）フィルタ
- レポート未検出時は `report_available: false` + 生成コマンドのヒントで非エラー応答（投機的呼び出し安全）

**入力**

```ts
{
  path?: string;            // 対象ファイル/ディレクトリ
  uncovered_only?: boolean; // 未カバーシンボルのみ
  threshold?: number;       // この率（%）未満のシンボルのみ
}
```

**出力**

```ts
{
  report_available: boolean;
  format?: "lcov" | "cobertura" | "coveragepy" | "llvm-cov";
  overall_pct?: number;
  files?: Array<{
    path: string;
    pct: number;
    symbols: Array<{ name: string; line: number; covered: number; total: number; pct: number }>;
  }>;
  hint?: string;            // report_available=false 時の生成コマンド例
  token_count: number;
}
```

---

### 2.30 コードオーナーシップ系（新規）

#### 2.30.1 `read_code_ownership`

`git log` / `git blame` を融合し、churn（変更頻度ホットスポット）・主要オーナー・最終更新を集約して返す。「なぜこうなったか」「誰に聞くべきか」を 1 コールで把握する分析ツール。

- ファイルごとに: コミット数（churn）・直近更新日・著者別行数シェア上位・主要オーナー
- `path` 絞り込み・`top_n`（ホットスポット上位件数）・`since`（期間限定）
- `read_git_log` / `read_git_blame_body` の上位レイヤとして実装

**入力**

```ts
{
  path?: string;
  top_n?: number;   // ホットスポット上位（デフォルト 20）
  since?: string;   // 例 "3 months ago"
}
```

**出力**

```ts
{
  hotspots: Array<{
    path: string;
    commits: number;       // churn
    last_modified: string;
    primary_owner: string;
    owners: Array<{ author: string; pct: number }>;
  }>;
  token_count: number;
}
```

---

### 2.31 依存監査系（新規）

#### 2.31.1 `read_dependency_audit`

依存パッケージの既知脆弱性を check-only でスキャンし、構造化サマリを返す。`read_security_surface`（コード側）に対する依存側のセキュリティ補完。

- 駆動: `npm audit --json` / `cargo audit --json` / `pip-audit -f json` / `osv-scanner --json`。マニフェスト/ロックファイルから生態系を自動判別
- 出力はパッケージ・severity・CVE/Advisory ID・影響バージョン・修正バージョンに正規化し、severity 降順でソート
- ツール未導入時は `scanner_available: false` + インストールヒントで非エラー応答（`read_type_diagnostics` と同方式）

**入力**

```ts
{
  severity?: "low" | "moderate" | "high" | "critical";  // この重大度以上のみ
  max_items?: number;
}
```

**出力**

```ts
{
  scanner_available: boolean;
  ecosystem?: "npm" | "cargo" | "pip" | "osv";
  vulnerabilities?: Array<{
    package: string;
    severity: string;
    id: string;             // CVE / RUSTSEC / GHSA
    affected: string;
    patched?: string;
    title: string;
  }>;
  hint?: string;
  token_count: number;
}
```

---

### 2.32 公開API系（新規）

#### 2.32.1 `read_api_surface`

外向きに公開されたシンボル（`pub` / `export` / `__all__` 等）のみを抽出し、ライブラリの外部境界を返す。利用側理解・破壊的変更検知に用いる。`diff_schemas` と組み合わせれば public API 差分（semver 違反警告）へ発展可能。

- tree-sitter の可視性判定で公開項目のみフィルタ。シグネチャのみ（本文なし）
- 対応: Rust（`pub`/`pub(crate)` 区別）/ TS・JS（`export`）/ Python（`__all__` ＋ 非アンダースコア top-level）/ Go（大文字始まり）
- `path` 絞り込み・`include_crate_visible`（`pub(crate)` 等を含めるか）

**入力**

```ts
{
  path?: string;
  include_crate_visible?: boolean;  // pub(crate) 等の準公開も含める
}
```

**出力**

```ts
{
  api: Array<{
    path: string;
    language: string;
    items: Array<{ kind: string; name: string; signature: string; visibility: string }>;
  }>;
  token_count: number;
}
```

---

### 2.33 自動ズーム（既存ツール拡張）

#### 2.33.1 `read_code_body` / `read_code_skeleton` の `zoom: auto`

`check_budget` のステータス（normal / conservative / aggressive / critical）に応じて、コード読み取りツールが skeleton ↔ sketch ↔ body のズームレベルを自動選択する。エージェントが明示指定しなくても予算に応じて最適化される。

- `zoom: "auto"` 指定時、現在のバジェットステータスを参照して返却粒度を決定（critical→skeleton、aggressive→sketch、normal→body）
- 返却時に `zoom_applied` で実際に採用したレベルを通知
- バジェット情報はセッション内に保持（`check_budget` 呼び出しで更新）

---

### 2.34 MCP リソース公開（プロトコル拡張）

#### 2.34.1 MCP Resources

主要ファイルを MCP `resources` として公開し、対応クライアントが `resources/list`・`resources/read`・変更通知を通じて能動取得できるようにする。

- `ServerHandler` の `list_resources` / `read_resource` を実装。エントリポイント・マニフェスト・README 等を URI（`t0k3n://<path>`）で公開
- デルタ基盤（`content_ledger` / mtime）と連携し、未変更リソースは差分通知のみ
- 公開対象は `project_digest` のエントリポイント判定ロジックを流用

---

### 2.35 オプトイン書き込みツール群（Phase 14）

ソース変更ツールはこれまで `patch_symbol`（更新）・`rename_symbol`（リネーム）のみで、**Create / Delete・ファイル新規作成・一括書き込みが欠落**していた。シンボル CRUD を完結させ、安全性のため**読み取り専用がデフォルト・書き込みはオプトイン**とする。

#### 2.35.0 `--enable-writes` ゲート

`read_type_diagnostics` の `--enable-diagnostics` 機構を踏襲。新規書き込みツール（`create_file` / `delete_symbol` / `insert_symbol` / `apply_edits`）は `--enable-writes` または `T0K3N_ENABLE_WRITES=1` のときのみ `ToolRouter` に登録。デフォルトは非登録・呼び出し不可。既存の `patch_symbol` / `rename_symbol` は後方互換のためゲート外（常時有効）。`debug_info` に `writes_enabled` を追加。

全書き込みツール共通の house rules: `dry_run` プレビュー・行番号陳腐化ガード（`expected_name`）・CRLF/末尾改行保持・出力は diff/サマリのみ（全文を返さない）。

#### 2.35.1 `create_file`

ファイル新規作成（従来は `run_command` 頼みだった欠落を解消）。`overwrite` デフォルト false で上書き拒否、親ディレクトリ自動作成、`safe_path` でトラバーサル防止。

```ts
{ path: string; content: string; overwrite?: boolean; dry_run?: boolean }
→ { path, bytes, created, overwritten, written }
```

#### 2.35.2 `delete_symbol`

スケルトン ID 指定でシンボルを削除（`read_dead_code` の書き込み対）。範囲＋末尾空行1行をクリーン除去。`parse_id`・`expected_name` 陳腐化検知・CRLF 保持を `patch_symbol` から流用。

```ts
{ path: string; id: string; expected_name?: string; dry_run?: boolean }
→ { removed_lines, diff, written }
```

#### 2.35.3 `insert_symbol`

tree-sitter ではなく構造的位置指定でコード挿入。`patch_symbol`(更新)＋`insert_symbol`(挿入)＋`delete_symbol`(削除)で CRUD 完結。前後に空行を自動付与。

```ts
{ path: string; content: string; mode: "after_symbol"|"before_symbol"|"after_imports"|"end_of_file"; anchor_id?: string; dry_run?: boolean }
→ { inserted_at_line, diff, written }
```

#### 2.35.4 `apply_edits`

複数ファイルへの `{path, find, replace}` を1コールで**アトミック**適用（`batch_read` の書き込み対）。各 find はファイル内一意必須（曖昧時は候補行番号エラー）。1つでも失敗すれば何も書かない。出力は各編集の行＋before/after サマリのみ。

```ts
{ edits: Array<{ path: string; find: string; replace: string }>; dry_run?: boolean }
→ { files_changed, edits_applied, changes: Array<{path, line, before, after}>, written }
```

---

### 2.36 構造差分（既存ツール拡張・Phase 16）

#### 2.36.1 `read_git_diff` の `zoom`

`read_code_body` で確立した段階ズーム（skeleton ↔ sketch ↔ body ↔ auto）を **diff（変化）** に適用する。状態の読み取りは構造化済みだが、変化は行ベースのままだった非対称を埋める。

- `zoom: "body"`（既定）— 従来どおり完全な unified diff（`--unified=2`）
- `zoom: "sketch"` — ファイルヘッダ＋ハンクヘッダ（`@@ ... @@ <関数コンテキスト>`）のみ。`+`/`-` 本文行を全削除（`--unified=0`）
- `zoom: "skeleton"` — ファイル × 囲みシンボル単位に集約し、`{symbol, hunks, added, deleted}` の件数だけ返す。diff 本文ゼロ。囲みシンボル名は git ハンクヘッダの関数コンテキスト（xfuncname）を流用、コンテキスト無しは `(top-level)`
- `zoom: "auto"` — 既存 `zoom_level()`/`budget_status` を参照（critical→skeleton、aggressive→sketch、それ以外→body）
- 返却時に `zoom_applied` で採用レベルを通知。`stat_only` 指定時は従来どおり（zoom 無視・`zoom_applied: "body"`）
- ファイルステータス（added / deleted / renamed / modified）を判定して付与

```ts
{ base?: string; path?: string; stat_only?: boolean; zoom?: "body"|"sketch"|"skeleton"|"auto" }
→ { diff, files?: Array<{path, status, added, deleted, symbols: Array<{symbol, hunks, added, deleted}>}>, zoom_applied, token_count }
```

用途: 大型 PR を `skeleton` で俯瞰（どのファイルのどのシンボルが何行変わったか）→ 怪しい箇所だけ `body` で精読、という「構造を先に、必要な所だけ」の段階読みを変化に対しても実現する。

---

### 2.37 root 未設定時のツール呼び出しごとオーバーライド（既存・Phase 17）

`--root` / `T0K3N_ROOT` を一切設定せずにサーバーを起動した場合、従来はプロセスの
カレントディレクトリにフォールバックし、意図したワークスペースと一致しないことが
あった。`EffectiveRoot`（`rmcp::handler::server::tool::FromToolCallContextPart` 実装）
を全 `#[tool]` ハンドラ（root を使うもの）の引数に追加し、`root_configured == false`
の間だけ呼び出し引数の `root`（絶対パス文字列）を採用、`true` の間は常に起動時の
root を優先する。

- root を消費する抽出は `Parameters<T>` より前に実行する必要がある（`Parameters<T>` が
  `arguments` を `take()` してしまうため）。`EffectiveRoot` を `Parameters<T>` の前に
  置くことで `arguments` から `root` キーを事前に取り除き、各ツール固有の
  `Parameters` 構造体には影響を与えない
- 純粋なロジック（`resolve_effective_root`）を抽出し、`ToolCallContext` 構築不要で
  ユニットテスト可能にした
- `root` 引数は各ツールの JSON Schema には現れない（`Parameters<T>` 由来のスキーマ
  生成の対象外のため）。代わりに `get_info` の `instructions`（root 未設定時のみ
  追記）と `debug_info` の `root_configured` フィールドでクライアントに通知する
- リソース系（`list_resources`/`read_resource`）は MCP Resources プロトコルの対象で
  ツール呼び出しではないため対象外。`memory_*`/`task_*`/`session_*` は起動時に固定
  された DB 接続を使うため root 非依存で対象外

```ts
// 例: read_directory_tree の呼び出し引数（root 未設定時のみ有効）
{ root?: string, ...通常のパラメータ }
```

---

### 2.38 `write_markdown_section`（Markdown構造書き込み・Phase 18）

`read_markdown_toc` / `read_markdown_section` には読み取り対のみあり、**Markdown/ドキュメント系の書き込みツールが欠落**していた。`patch_symbol`（更新）・`insert_symbol`（挿入）・`delete_symbol`（削除）の設計をスケルトン ID ではなく Markdown の見出しアンカーに適用し、1ツールで CRUD を完結させる。`--enable-writes` ゲート配下。

`mode` ごとの挙動:

- `replace` — `anchor` で指定した見出し〜次の同レベル以上の見出し直前までを `content`（見出し行含む）で丸ごと置換
- `insert_before` / `insert_after` — `anchor` のセクション境界の直前/直後に `content` を新規ブロックとして挿入（前後の空行は自動付与）
- `append` — ファイル末尾に `content` を追加（`anchor` 不要）
- `delete` — `anchor` のセクションを末尾の空行1行ごと削除

`expected_title` を渡すと実際の見出しテキストと比較し、不一致なら拒否する（`patch_symbol`/`delete_symbol` の `expected_name` と同じ陳腐化ガード）。`read_markdown_section`/`extract_sections` と同じ「次の同レベル以上の見出しで打ち切る」境界判定を共有するため、`scan_headings`/`HeadingLine` を `markdown.rs` から `pub(crate)` で再利用する。house rules（`dry_run` プレビュー・CRLF/末尾改行保持・出力は diff のみ）に準拠。

```ts
{
  path: string;
  mode: "replace" | "insert_before" | "insert_after" | "append" | "delete";
  anchor?: string;       // append 以外で必須
  content?: string;      // delete 以外で必須
  expected_title?: string;
  dry_run?: boolean;
}
→ { diff, written }
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
| `read_code_sketch` | Rust 実装 | skeleton と body の中間ズーム（制御フロー骨格＋呼び出し行を残し純データ行を畳む。body 比 60〜70% 削減） |
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
| `patch_symbol` | Rust 実装 | skeleton ID 指定でシンボル本文を置換（全文ロード不要の書き込み） |
| `read_context_pack` | Rust 実装 | タスク記述からランク付きファイル+シンボル+本文をバジェット内で1コール収集 |

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

### Proto 系

| ツール | 種別 | 説明 |
|---|---|---|
| `read_proto_schema` | Rust 実装 | Protocol Buffers スキーマのメッセージ/サービス一覧 |
| `read_proto_type` | Rust 実装 | メッセージ/サービス名指定でフィールド定義詳細取得 |

### Notebook 系

| ツール | 種別 | 説明 |
|---|---|---|
| `read_notebook_cells` | Rust 実装 | Jupyter ノートブックのセル一覧（種別・コード・出力サマリ） |
| `read_notebook_cell` | Rust 実装 | セル番号指定で本文・出力全取得 |

### ログ系

| ツール | 種別 | 説明 |
|---|---|---|
| `read_log_tail` | Rust 実装 | ログファイル末尾取得（レベル/パターンフィルタ） |
| `read_stack_trace` | Rust 実装 | スタックトレース→ソースコンテキスト自動解決 |

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
| `delta_reset` | Rust 実装 | デルタリード台帳のクリア（次回readで全文返却） |

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
| `help` | Rust 実装 | カテゴリ別ツール探索（AI がどのツールを使うべきか不明な時に呼ぶ） |

### 分析系（Phase 5）

| ツール | 種別 | 説明 |
|---|---|---|
| `read_complexity_map` | Rust 実装 | 関数ごとの循環的複雑度（low/medium/high/critical）・リファクタ候補優先順位付け |
| `read_dead_code` | Rust 実装 | 未使用シンボル検出（コンパイラ・LSP 不要・全言語対応） |
| `read_refactor_impact` | Rust 実装 | リファクタブラスト半径：呼び出し元・参照ファイル・テスト一覧を 1 コールで |
| `read_security_surface` | Rust 実装 | 静的セキュリティサーフェス（injection / XSS / secrets / unsafe / path_traversal） |
| `diff_schemas` | Rust 実装 | git ref 間スキーマ差分（OpenAPI / Prisma / SQL / TypeScript 型） |
| `read_pr_context` | Rust 実装 | PR 文脈一括ロード（変更ファイルスケルトン + テスト発見 + コミット一覧） |

### コマンド実行系（Phase 6）

| ツール | 種別 | 説明 |
|---|---|---|
| `run_command` | Rust 実装 | コマンド実行・スマートフィルタリング（成功: 末尾サマリ / 失敗: エラー行+警告行） |

### LSP / 型診断系（Phase 12）

| ツール | 種別 | 説明 |
|---|---|---|
| `read_type_diagnostics` | Rust 実装 | LSP 相当の静的型診断（cargo check / tsc / pyright・mypy / go vet を check-only 駆動し構造化診断を返す） |

### プロジェクトダイジェスト系（Phase 11）

| ツール | 種別 | 説明 |
|---|---|---|
| `project_digest` | Rust 実装 | git HEAD で無効化されるキャッシュ済みアーキテクチャ要約（言語統計＋エントリポイント＋ツリー）を ~2k トークンで返すウォームスタート |

### 拡張・書き込み系（Phase 13）

| ツール | 種別 | 説明 |
|---|---|---|
| `rename_symbol` | Rust 実装 | シンボルを全ファイル横断で安全リネーム。影響行サマリのみ返す書き込み系 |
| `read_test_coverage` | Rust 実装 | lcov/cobertura/coverage.py/llvm-cov をシンボル単位でマッピング。未カバー領域を可視化 |
| `read_code_ownership` | Rust 実装 | git log/blame 融合。churn・主要オーナー・最終更新を集約 |
| `read_dependency_audit` | Rust 実装 | npm/cargo/pip/osv audit を check-only 駆動し脆弱性を正規化 |
| `read_api_surface` | Rust 実装 | pub/export/__all__ の公開シンボルのみ抽出（外部境界） |
| `read_code_body`（拡張） | Rust 実装 | `zoom: auto` で予算に応じ skeleton↔sketch↔body 自動選択 |
| MCP Resources | プロトコル | 主要ファイルを `t0k3n://` リソースとして公開・差分通知 |

### オプトイン書き込み系（Phase 14）— `--enable-writes` 必須

| ツール | 種別 | 説明 |
|---|---|---|
| `create_file` | Rust 実装 | ファイル新規作成。上書き拒否デフォルト・親ディレクトリ自動作成 |
| `delete_symbol` | Rust 実装 | スケルトン ID でシンボル削除（`read_dead_code` の対） |
| `insert_symbol` | Rust 実装 | 構造的位置へコード挿入（after_symbol/before_symbol/after_imports/end_of_file） |
| `apply_edits` | Rust 実装 | 複数ファイルへの find/replace をアトミック適用（`batch_read` の対） |

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
- [x] ダッシュボード強化（ツール使用統計・累計トークン節約量の可視化）
- [x] ダッシュボード リリース・パッチノート表示（`/api/releases` 経由で T0K3N-MCP 自身（`tonrakun/t0k3n-mcp`）の GitHub Releases を取得・表示。`--root` で指定した対象プロジェクトのディレクトリには依存しない）

### Phase 5 — 差別化分析ツール v2.4+

- [x] `read_complexity_map`（関数ごとの循環的複雑度・リスクレベル分類・コンパイラ不要）
- [x] `read_dead_code`（未使用シンボル検出・全言語対応・LSP 不要）
- [x] `read_refactor_impact`（リファクタブラスト半径分析：call_graph + symbol_usages + テスト検出を 1 コールで）
- [x] `read_security_surface`（静的セキュリティサーフェス：injection / XSS / secrets / unsafe / path_traversal）
- [x] `diff_schemas`（OpenAPI / Prisma / SQL / TypeScript の git ref 間スキーマ差分）
- [x] `read_pr_context`（PR 文脈一括ロード：変更ファイルのスケルトン + テスト + コミット一覧）

### Phase 6 — コマンド実行 v2.5+

- [x] `run_command`（コマンド実行＋スマートフィルタリング：成功時サマリ / 失敗時エラー行・警告行抽出）

### Phase 7 — ディスカバリ v2.6+

- [x] `help`（カテゴリ別ツール探索・instructions 肥大化防止）
- [x] instructions を MANDATORY SUBSTITUTIONS + `help` 呼び出し指示のみにスリム化
- [x] `help` 引数なし時はカテゴリ名一覧のみ返すよう変更（全カタログは `help("all")`、ツール説明・スキーマ説明も追従）
- [x] instructions を原則駆動に書き直し（代替表を廃止し、フル読み禁止 / skeleton→body / check_budget / batch_read / help 案内の5原則 + DELTA READS 維持。500トークン以内）

### Phase 8 — トークン削減第2世代 v2.7+

「1回の読み取りを小さくする」最適化に加え、(1) 再読の抑制、(2) JSON 構造オーバーヘッド、(3) 書き込み側、(4) 探索往復回数 を削減対象に拡張する。

- [x] markdown セクション抽出バグ修正（次見出しで打ち切られず EOF まで返る問題・インラインコード見出しの不一致。`read_markdown_section` / `read_webpage_section` 両方に影響）
- [x] コンパクト出力フォーマット（デフォルト有効・`--format json` で従来出力）
  - 全ツール共通の出口（`ok_json`）で汎用レンダリング: 同種オブジェクト配列→パイプ区切りテーブル（キーは1回のみ）、null/空コンテナ省略、複数行文字列→インデントブロック
  - レスポンスあたり 20〜40% 削減、ツール個別変更ゼロ
- [x] デルタリード（セッション読み取り台帳）
  - 高頻度 read 系 10 ツールで tool+params をキーに前回送信内容を記録
  - 同一内容の再読 → 約50トークンの `unchanged` スタブ / 変更あり → unified diff（全文の60%以下の場合のみ）
  - `delta_reset` ツールで台帳クリア（コンテキスト圧縮後など）
- [x] `patch_symbol`（シンボル単位書き込み）
  - skeleton ID の行範囲を新しい本文で置換。skeleton → body 読み → body 書きが全文ロードなしで完結
  - `expected_name` による陳腐化検知、`dry_run` プレビュー、CRLF/末尾改行保持
- [x] `read_context_pack`（タスク駆動一括コンテキスト収集）
  - クエリの字句スコアリング（パス・内容・シンボル名/シグネチャ）でファイル/シンボルをランク付けし、トークンバジェット内に貪欲詰め（ランキング+シグネチャ常時、本文は上位から）
  - 探索フェーズの tree → search → skeleton → body 往復を 1 コールに置換

### Phase 9 — 保守・インストーラ改善 v2.5.0

- [x] スコープ付きディレクトリ走査の Windows バグ修正
  - `safe_path` の canonicalize が返す `\\?\` verbatim パスにより `strip_prefix(root)` が常に失敗し、`read_complexity_map` / `read_dead_code` が 0 件、`read_code_deps` の imported_by が常に空になっていた
  - `security::scoped_root` / `security::rel_display` 共通ヘルパー導入（20 箇所超の重複 strip_prefix チェーンを集約）
  - `safe_path` の `..` アンダーフロー拒否（相対 root 時に traversal チェックが素通りだった穴を `PathTraversal` エラーで防止）
- [x] clippy 全警告解消（let-chains 化・`sort_by_key(Reverse)` 等）
- [x] `--version` / `-V` フラグ（ログ・サーバー起動前に即出力。インストーラの検証用）
- [x] インストールスクリプト UX 改善（install.ps1 / install.sh）
  - 4 ステップ進捗表示（最新リリース確認 → 導入済みバージョン確認 → ダウンロード → インストール+検証）
  - 最新版導入済みならスキップ・ダウンロードサイズ検証・更新前後バージョンのサマリ表示
  - `INSTALL_DIR/VERSION` ファイルでバージョン管理（旧バイナリのプローブは 5 秒タイムアウトでガード）
- [x] Windows アップデート失敗修正
  - MCP サーバー実行中は exe がロックされ `Remove-Item` が必ず失敗していた → リネーム方式スワップに変更（実行中でもリネームは可能）。旧バイナリは次回実行時にクリーンアップ

### Phase 10 — CLI サブコマンド化・バイナリ名変更 v2.6.0

- [x] バイナリ名を `t0k3n` に変更（パッケージ名は `t0k3n-mcp` のまま）。リリース成果物も `t0k3n-*` に改名
- [x] `t0k3n upgrade` サブコマンド（自己更新）
  - GitHub 最新リリースの取得 → semver 比較 → プラットフォーム別成果物のダウンロード → サイズ検証 → 置換
  - Windows はリネーム方式スワップ（実行中でも更新可能）、Unix は rename(2) でアトミック置換
  - 同ディレクトリに旧名 `t0k3n-mcp` バイナリがあれば一緒に更新（既存 `.mcp.json` 互換維持）
- [x] `t0k3n setup [dir]` サブコマンド（`.mcp.json` の生成・既存設定へのマージ）
  - [x] `--root` は必須指定のため、setup 実行ディレクトリ（または指定 dir）の絶対パスを `args` に常に出力
- [x] `t0k3n version` / `t0k3n help`
- [x] インストールスクリプトを薄いブートストラップに縮小（ダウンロード → 配置 → ユーザー PATH 追加のみ。昇格不要。更新は `t0k3n upgrade` に移譲）
- [x] アップデート通知を `t0k3n upgrade` の案内に変更
- [x] リリース CI の GitHub Actions を Node 24 対応メジャーへ更新（checkout v6 / cache v5 / upload-artifact v7 / download-artifact v8 / action-gh-release v3。2026-06-16 の Node 24 強制デフォルト化対応）

### Phase 11 — トークン削減第3世代 v2.7+

第2世代までは入力側（読み取り）の削減が中心。第3世代は (1) 出力トークン（モデルが書く側・入力の約5倍単価）、(2) ツール横断の重複送信、(3) コマンド出力の再送、(4) セッション開始時の探索フェーズ を削減対象とする。詳細はタスク DB（task_list tag=gen3-token-reduction、ID 19〜24）参照。

- [x] `patch_symbol` 編集スクリプトモード（タスク19）— `edits: [{find, replace}]` でシンボル本文の部分編集。new_body と排他。find はシンボル内で一意であれば良く（ファイル全体で一意である必要なし）、曖昧時は該当行番号つきでエラー。順次適用・CRLF/末尾改行保持・dry_run/expected_name は既存機構を流用
- [x] `run_command` デルタモード（タスク20）— 同一コマンド（command+cwd キー）の再実行時にエラー/警告の差分のみ返す（新規分の本文 + 解消/不変は件数のみ）。summary は pass/fail 反転時、または成功時に内容が変わった場合のみ再送。`delta_reset` でコマンド台帳もクリア対象に
- [x] クロスツール送信済みコンテンツ台帳（タスク21）— `ContentLedger`。デルタ台帳（tool+params キー）に加え、ファイル+行範囲（`path#id`）をキーに送信済み本文を記録する第2台帳を導入。`read_context_pack` が送った本文を `read_code_body` で再要求した場合等、ツール横断の重複送信を `path:start-end` 参照スタブに置換。失効は mtime + コンテンツハッシュの一致でのみ再利用（編集による行範囲ずれを防止）。`delta_reset` のクリア対象に含める
- [x] `read_code_sketch`（タスク22）— skeleton と body の中間ズーム。skeleton ID を受け取り、シグネチャ＋分岐/ループ＋ブロック区切り＋呼び出し行をそのまま残し、純データ行（代入・リテラル）の連続を `… N lines …` に畳む。body 比 60〜70% 削減。行ベースのヒューリスティック（言語別コメントトークン対応）で全言語横断・純関数化しユニットテスト
- [x] プロジェクトダイジェスト（タスク23）— `project_digest`。git HEAD で無効化されるキャッシュ済みアーキテクチャ要約（git HEAD・言語別統計・エントリポイントファイルと上位シンボル・浅いディレクトリツリー）を ~2k トークンで 1 コール返却。`.t0k3n/digest.json` に HEAD キーでキャッシュし HEAD 変化時に自動再生成。`refresh:true` で強制再生成・`dirty` で未コミット作業ツリーを通知。read_workspace_stats / read_directory_tree / read_code_skeleton を再利用
- [x] `batch_read` テンプレート因数分解（タスク24）— `factor: true` で類似結果（マイグレーション・fixture 等）を正規形 1 つ + 各ファイルの unified diff にまとめて返す。類似度は `similar` の行ベース ratio（閾値 0.5）で判定、diff が本文より小さい場合のみ採用。因数分解された結果は `{template_ref, diff}` ＋ `template_ref` フィールドを持ち、`factored` 件数を返す

### Phase 12 — LSP / 型診断 v2.7+

`run_command` で型チェッカーを生実行すると冗長なコンパイラ出力がそのまま渡る。言語ネイティブの診断エンジンを check-only で駆動し、LSP 相当の構造化診断のみをトークン効率よく返す補助ツールを追加する。

- [x] `read_type_diagnostics`（タスク25）— LSP 相当の静的型診断。`cargo check --message-format=json`（Rust）/ `tsc --noEmit`（TS）/ `pyright`・`mypy`（Python）/ `go vet`（Go）を check-only 駆動し `{file, line, col, severity, code, message}` を返す。言語自動判別・severity / max_items / path フィルタ・重複排除・重要度ソート。チェッカー未導入時は `checker_available: false` + インストールヒントで非エラー応答（投機的呼び出し安全）。パーサは純関数化し各言語の実出力でユニットテスト
- [x] `read_type_diagnostics` をオプトイン化（重量級化の回避）— `--enable-diagnostics` / `T0K3N_ENABLE_DIAGNOSTICS=1` で起動時のみ `ToolRouter` に登録。デフォルトはツール一覧非表示・呼び出し不可。`debug_info` に `diagnostics_enabled` を追加

### Phase 13 — 拡張・書き込み・gen4 v3.1+

書き込み系の拡充（`rename_symbol`）、テスト/セキュリティ/オーナーシップ分析の補完、トークン削減第 4 世代（自動ズーム）、および MCP リソース公開を行う。

- [x] `rename_symbol`（タスク27）— シンボルを全ファイル横断で安全リネーム。`read_symbol_usages` の検出基盤（WalkBuilder + `\bSYMBOL\b`）で識別子境界一致のみ置換（部分一致は非対象）し、`dry_run`・CRLF/末尾改行保持を `patch_symbol` から流用。`new_name` は識別子バリデーション。出力は影響ファイル数 + 各行 before/after のみで全文は返さない。注意: 文字列/コメント内の同名も置換しうるため dry_run プレビュー前提
- [x] デルタリード第 4 世代 — セッション横断の永続台帳（タスク28）。`content_ledger`（ファイル+行範囲+コンテンツハッシュ）を `.t0k3n/content_ledger.json` に原子的書き込みで永続化。**正しさ重視の設計**: 前セッション由来のヒットは `UnchangedColdCache` として「不変だが現コンテキストには無い／必要なら再読込」と正直に提示し、今セッション内ヒット（hot）のみ従来の `AlreadySent` を返す。mtime+hash でファイル単位失効、git HEAD は `debug_info` に表示。`delta_reset` は永続ファイルも消去
- [x] `read_test_coverage`（タスク29）— lcov（lcov.info / cargo-llvm-cov）/ coverage.py JSON / cobertura XML を解析し、`read_code_skeleton` のシンボル範囲に行カバレッジをマッピング。シンボル単位 covered/total/pct ＋ overall_pct。`uncovered_only`（pct<100）/`threshold` フィルタ。レポートは gitignore されがちなため慣習パスを明示探索。未検出時は `report_available:false` + 生成コマンドヒントで非エラー応答
- [x] `read_code_ownership`（タスク30）— `git log --numstat` 1パスで churn（コミット数）・最終更新日・著者別行貢献シェア（追加行ベース）を集約。churn 降順でホットスポット化。`path`/`top_n`/`since` 対応。バイナリ numstat（`-`）安全処理
- [x] `read_dependency_audit`（タスク31）— 生態系自動判別（Cargo.toml→cargo audit / package.json→npm audit / pyproject・requirements→pip-audit / go.mod→osv-scanner）し、`{package, severity, id, affected, patched, title}` に正規化。CVSS スコアは severity バケットへ変換。severity 降順ソート・`severity`（最小レベル）/`max_items` フィルタ。スキャナ未導入時は `scanner_available:false` + インストールヒントで非エラー応答。各フォーマットのパーサはユニットテスト済み
- [x] `read_api_surface`（タスク32）— 言語別の公開シンボル抽出。Rust `pub`（`pub(crate)` は `include_crate_visible` で区別）/ TS・JS `export` / Python `__all__` ＋ 非アンダースコア top-level / Go 大文字始まり。シグネチャのみ（本文なし）。`path` 絞り込み。`diff_schemas` と組み合わせて破壊的変更検知に発展可能
- [x] `check_budget` 連動の自動ズーム（タスク33）— `read_code_body` に `zoom`（body/sketch/skeleton/auto）を追加。`check_budget` 呼び出し時に strategy をサーバに保持し、`zoom:auto` で critical→skeleton / aggressive→sketch / それ以外→body を自動選択。採用レベルを `zoom_applied` で通知。マッピングは純関数 `zoom_level` に切り出しユニットテスト済み
- [x] MCP Resources 公開（タスク34）— `ServerHandler::list_resources`/`read_resource` を実装し、主要ファイル（マニフェスト・README・エントリポイント）を `t0k3n://<path>` URI で公開。`get_info` で `enable_resources()`。URI 解決は `safe_path` でトラバーサル防止、上限30件。`read_resource` は常に現在のディスク内容を返す

### Phase 14 — オプトイン書き込みツール群 v3.2+

シンボル CRUD を完結させる書き込みツールを追加。安全性のため**読み取り専用がデフォルト・書き込みはオプトイン**（`--enable-writes` / `T0K3N_ENABLE_WRITES=1`）。別 MCP サーバーに分けない理由: gen4 デルタ台帳のキャッシュ無効化（mtime+hash）はプロセス内でのみ整合し、書き込みが別プロセスだと「変更なし」誤判定で核心価値が崩れるため。tree-sitter・safe_path・dry_run 等の基盤も全面流用できる。

- [x] `--enable-writes` ゲート（タスク35）— `read_type_diagnostics` の opt-in 機構を踏襲。`WRITE_TOOLS`（create_file/delete_symbol/insert_symbol/apply_edits）を未有効時に `ToolRouter` から除去。`T0k3nServer::new` に `writes_enabled` 追加、`debug_info` に表示。既存 `patch_symbol`/`rename_symbol` は後方互換でゲート外
- [x] `create_file`（タスク36）— ファイル新規作成。`overwrite` デフォルト false で上書き拒否、親ディレクトリ自動作成、`safe_path` 防御、`dry_run`
- [x] `delete_symbol`（タスク37）— スケルトン ID でシンボル削除（`read_dead_code` の対）。範囲＋末尾空行1行を除去。`expected_name` 陳腐化検知・CRLF 保持
- [x] `insert_symbol`（タスク38）— 構造的位置へコード挿入（after_symbol/before_symbol/after_imports/end_of_file）。前後空行を自動付与。`patch_symbol`(更新)＋`insert_symbol`(挿入)＋`delete_symbol`(削除)で CRUD 完結
- [x] `apply_edits`（タスク39）— 複数ファイルへの find/replace をアトミック適用（`batch_read` の対）。find はファイル内一意必須・曖昧時は候補行番号エラー・1つでも失敗で何も書かない。出力は変更行サマリのみ

### Phase 15 — 書き込み第2弾（設定/インポート/整形/移動/安全機構）v3.2+

Phase 14 の書き込み基盤（`--enable-writes` ゲート・`writes.rs` 慣習）の上に、設定編集・import 管理・整形などの書き込みツールを追加。すべて `--enable-writes` ゲート配下、house rules（dry_run・CRLF/末尾改行保持・diff/サマリのみ出力）準拠。

- [x] `set_config_value`（タスク41）— JSON/YAML/TOML の dot-path に値書込（`read_json_yaml_value` の対）。中間オブジェクト自動生成・任意 JSON 型。`serde_json` の preserve_order で JSON キー順保持（YAML/TOML のコメントは best-effort で消える）。`json_yaml::{parse_file, tokenize_path}` と `writes::unified_diff` を流用。`old_value`/`new_value`/`diff` を返す
- [x] `manage_imports`（タスク42）— import 文の追加/削除/重複排除。whole-line ベースで言語非依存。`writes::import_boundary` で挿入位置決定、trimmed 一致で削除、既存＋add 内重複を skip。`added`/`removed`/`skipped`/`diff` を返す
- [x] `format_code`（タスク43）— 拡張子で rustfmt/prettier/black/gofmt を駆動。`diagnostics::{run_shell, looks_unavailable}` 流用、未導入は `formatter_available:false` + ヒントで非エラー。dry_run は `.t0k3n/fmt-tmp/` のコピーを整形して diff プレビュー（実ファイル不変）。整形前後の diff・changed を返す
- [x] `move_symbol`（タスク44）— シンボルを src から dest へ移動（dest 無ければ作成）。抜き出し（delete_symbol 相当）＋末尾追記。import 追従は best-effort（書き換えはせず、`read_symbol_usages` で参照ファイルを検出し warnings に列挙）。`symbol_name` で陳腐化ガード＋参照警告。src/dest の両 diff を返す
- [x] `edit_checkpoint`/`rollback`（タスク45）— 書込前スナップショットと巻き戻し。git 管理下は `git stash create`（作業ツリー不変）→ `git checkout <sha> -- .` で復元。非 git 時は gitignore 対応で `.t0k3n/checkpoints/<id>/` へコピー退避→復元。`checkpoint_id` は自己完結（git:/copy: プレフィックス）でサーバ状態不要。制約: チェックポイント後に新規作成されたファイルは rollback で削除されない

---

### Phase 16 — 構造差分（変化の段階ズーム）v3.3+

状態の読み取りは tree-sitter で構造化済みだが、変化（diff）は行ベースのままという非対称を解消する。`read_code_body` の段階ズームを diff に適用。

- [x] `read_git_diff` の `zoom`（タスク46）— `skeleton`（ファイル×囲みシンボルの+/-件数集約、本文0）/`sketch`（ファイル・ハンクヘッダのみ）/`body`（既定・完全diff）/`auto`（既存 `zoom_level()`/`budget_status` 流用）。囲みシンボルは git ハンクヘッダの関数コンテキストを流用、無しは `(top-level)`。`stat_only` は従来優先。`files`/`zoom_applied` を追加返却。ハンドラで `auto` を `resolve_zoom` 解決し具体レベルを stateless fn へ。大型 diff を俯瞰→精読する段階読みを変化に対して実現

---

### Phase 17 — root 未設定時のツール呼び出しごとオーバーライド v3.3+

`.mcp.json` 側で `--root`（または `T0K3N_ROOT`）を設定し忘れた／設定できない環境でも
サーバーを使えるようにする。

- [x] `EffectiveRoot` extractor（`root_configured` が false の間のみ呼び出し引数の
  `root` を採用、true の間は起動時 root を常に優先）。`#[tool]` ハンドラのうち
  root を使う 69 個に適用。`dedup_body` も root 引数を受け取るよう変更。
  `resolve_effective_root` を切り出しユニットテスト3件を追加。
  `get_info` instructions（root 未設定時のみ追記）と `debug_info.root_configured`
  でクライアントに状態を通知。`main.rs` に `T0K3N_ROOT` 環境変数フォールバックを追加

---

### Phase 18 — Markdown構造書き込み v3.4+

`read_markdown_toc` / `read_markdown_section` に書き込み対が無く、Markdown/ドキュメント系の編集は `apply_edits`（find/replace）や `create_file`（全文上書き）しか手段が無かったギャップを埋める。

- [x] `write_markdown_section`（タスク48）— 見出しアンカー基準の Markdown 書き込みツール（`read_markdown_toc`/`read_markdown_section` の対）。`mode`: `replace`（セクション丸ごと置換）/ `insert_before` / `insert_after`（セクション境界へ新規ブロック挿入）/ `append`（ファイル末尾追加）/ `delete`（セクション削除）。`expected_title` で TOC 陳腐化ガード。`markdown.rs` の `scan_headings`/`HeadingLine` を `pub(crate)` 化して境界判定（`read_markdown_section` と同じ「次の同レベル以上の見出しで打ち切る」ロジック）を共有。`--enable-writes` ゲート配下、house rules（dry_run・CRLF/末尾改行保持・diff のみ出力）準拠

---

### Phase 19 — 供給網・境界・出力精度の健全化 v3.4+

機能追加が Phase 18 まで積み上がった一方で、リリース経路・サンドボックス境界・
解析出力の精度といった「土台」が追いついていなかった。批判的レビューで洗い出した
以下の項目をまとめて解消する。

**CI / リリース**

- [x] `.github/workflows/ci.yml` を新設。push（main）/ PR / 手動で `cargo test`（3 OS）＋
  `cargo fmt --check`＋`cargo clippy --all-targets -D warnings`（ubuntu のみ）を実行。
  `cargo audit` は別ジョブで `continue-on-error`（上流 advisory で PR を止めないため可視化のみ）。
  従来は `release.yml` しか無く、テストがゲートされないままタグでバイナリが配布されていた
- [x] リポジトリ全体を `cargo fmt` で正規化（760 差分ハンク）。巻き戻し用に
  `.git-blame-ignore-revs` を追加し、`git blame` が整形コミットを飛ばせるようにした
- [x] `release.yml` に `SHA256SUMS.txt` 生成ステップを追加（アーティファクトを `dist/` に
  フラット化して `sha256sum *`）。リリース資産として公開
- [x] `t0k3n upgrade` に SHA256 検証を追加。バイナリより**先に**マニフェストを取得し、
  該当アーティファクトの記載が無い／ダイジェスト不一致ならインストールを拒否
  （`expected_sha256` は 64 桁 hex 以外の行を弾く。従来は「1MB 以上か」しか見ていなかった）
- [x] `install.sh` / `install.ps1` にも同じマニフェスト検証を追加（`sha256sum`/`shasum`/`Get-FileHash`）

**ケイパビリティモデルの一貫化**

- [x] `ServerConfig` 構造体を導入し `T0k3nServer::new(config, dashboard)` に集約
  （引数 7 個化と `clippy::too_many_arguments` を回避、以後の追加で全呼び出し側を触らない）
- [x] `run_command` を `COMMAND_TOOLS` としてオプトアウト化（`--disable-commands` /
  `T0K3N_DISABLE_COMMANDS=1`）。デフォルト有効なので後方互換。これにより
  「読み取り＝常時／シェル＝既定有効・除去可／構造化書き込み＝既定無効・追加可／型診断＝既定無効・追加可」
  という 4 段のケイパビリティが揃った
- [x] 「`--enable-writes` を付けなければ読み取り専用」という**誤った説明を全面撤回**。
  `run_command` が登録されている間はシェル経由で何でも書けるため、ゲートが守るのは
  ツール表面とスキーマ量であってマシンではない旨を `print_help` / `get_info` /
  README 両言語に明記
- [x] `--no-update-check` / `T0K3N_NO_UPDATE_CHECK=1` を追加。起動時のリリース確認は
  サーバーが自発的に行う唯一の外向き通信であり、閉域・監査環境では無効化手段が必要

**トークン削減の自己矛盾の解消**

- [x] `--tools <categories>` / `T0K3N_TOOLS` を追加。`help()` カタログのカテゴリ単位で
  登録ツールを絞る。ツールスキーマはクライアントが毎リクエスト運搬するため、
  ツールを減らすこと自体がトークン削減になる（91 ツール分のスキーマが常時コストだった）。
  未知カテゴリは起動時に exit 2 で拒否、`help`/`debug_info` は常に残す（`ALWAYS_KEEP_TOOLS`）。
  カテゴリ名は `help()` カタログから導出するので定義がドリフトしない
- [x] `get_info` instructions のツール数・カテゴリ数を実行時計算に変更（`91` のハードコードを撤去）。
  「サーバーを信頼せよ」と書いてある文章自体が陳腐化するのを防ぐ
- [x] README のツール数見出しが `REGISTERED_TOOLS.len()` と一致することを検証する
  `readme_tool_counts_match_the_registry` テストを追加（help カタログの staleness テストと同趣旨）

**サンドボックス境界**

- [x] `safe_path_or_absolute` の「絶対パスは無条件許可」を撤回。`convert_document` が
  システム一時ディレクトリに書く `t0k3n-*` スクラッチファイル（親ディレクトリを
  canonicalize 比較、`..` を含むパスは拒否）のみを例外とし、それ以外の絶対パスは
  root 内解決を要求。`SecurityError::AbsoluteNotAllowed` を追加して理由を明示。
  従来は `read_markdown_toc`/`read_markdown_section` だけが root 外を読める非対称だった

**解析出力の精度（誤検知）**

- [x] `read_security_surface` に `confidence`（実際に問題である確度）を追加し、
  `severity`（実際に問題だった場合の影響）と役割を分離。ルール表を `rule!` マクロ化して
  全 54 ルールに付与。`confidence` 降順→`severity` 降順でソートし、`min_confidence`
  パラメータで低信号を切れるようにした。結果に `by_confidence` と恒久 `note`（ヒューリスティックである旨）を追加
- [x] 文字列リテラル内マッチの抑制。`(`/`=`/`::` を含む「コードとしてのみ意味を持つ」
  パターンが引用符内に**しか**現れない行は、パターンが*使われている*のではなく
  *名前として書かれている*（ルール表・エラーメッセージ）と判定して除外。
  `../` や `-----BEGIN` のような内容パターンは対象外（リテラル内にあるのが正常）。
  自プロジェクトをスキャンした際に `security_surface.rs` 自身のルール定義が
  high severity として 10 件報告される、という自己言及的な誤検知を解消
- [x] テストコードの除外（既定）。Rust の `#[cfg(test)]` 以降を打ち切り、
  `tests`/`__tests__`/`spec`/`fixtures`/`testdata` セグメントや `*_test.go`/`*.spec.ts` 等の
  パスをスキップ。`include_tests: true` で従来動作。テストフィクスチャは誤検知の最大要因
- [x] 単語境界の要求。識別子で始まるパターンは識別子境界から始まることを要求し、
  `system(` が `detect_ecosystem(` にマッチする類の部分文字列誤検知を解消
- [x] 抑制マーカーを追加。`t0k3n:ignore-security-scan`（ファイル先頭 20 行以内でファイル全体を除外）と
  `t0k3n:ignore-security`（行単位で除外）。ヒューリスティックには還元不能な誤検知
  （パターン表・セキュリティテストのフィクスチャ・意図的なサンプル）が残るため、
  毎回トリアージし直すのではなくソース側で表明できる必要がある。
  `security_surface.rs` 自身にファイルマーカーを付与した（攻撃パターンの表を持つファイルは
  構造的に自分の表にマッチする）
- [x] PEM ルールを `-----BEGIN`→`-----BEGIN `（末尾スペース）に厳格化。実際の PEM ヘッダは
  必ず `-----BEGIN <TYPE>-----` の形をとるため、マーカーを列挙しているだけの行に反応しなくなる
- [x] 効果測定（自プロジェクトを新バイナリでスキャン）: 40 件 → 29 件、
  自己参照の誤検知 10 件 → 0 件、`min_confidence:"medium"` では 0 件
  （このリポジトリは git/シェルを正当に起動するため、残る 29 件が全て low confidence なのが正しい）
- [x] `read_dead_code` / `read_security_surface` のツール説明に「ヒューリスティックであり
  `confidence` を確認してから報告せよ」を明記し、`get_info` instructions にルール 7 として追加

**その他**

- [x] デルタリードの `unchanged` スタブに `content_sha256`（12 桁）を追加。
  コンテキスト圧縮で内容を失ったエージェントが「変わっていない」を検証できないという
  静かな失敗モードに対する自己照合手段。note も「推測するな、`delta_reset` を呼べ」に強化
- [x] ダッシュボードにプロセスごとのアクセストークンを導入。`/api/*` と `/ws` は
  クエリ `t=<token>` を要求（比較は定数時間寄り）。トークンは起動時ログの URL と
  `--open-browser` の URL に含まれ、HTML 側は `location.search` から引き継ぐ。
  ループバックバインドだけでは同一マシンの他ユーザー／他プロセスから
  呼び出しログ（パス・シェルコマンド）を隠せないため
- [x] `semantic_search` のツール説明に「別プロセスの `claude -p` を起動する = それ自体が
  課金対象のモデル呼び出しであり、レイテンシと非決定性を伴う。grep の代替ではない」を明記
- [x] `serde_yaml` 0.9（アーカイブ済み）を維持フォーク `serde_yaml_ng` 0.10 へ移行。
  Cargo のパッケージリネームでエイリアスし、20 箇所の呼び出し側は無変更
- [x] ツールハンドラ境界での panic 隔離。`instrument!` マクロ（全ハンドラが通る唯一の経路）で
  `AssertUnwindSafe` + `futures_util::FutureExt::catch_unwind` し、panic をツールエラーへ変換
  （`panic_to_error`）。長寿命の stdio プロセスである以上、1 ツールの panic で
  編集セッション全体を落としてはならない。`src` 配下 337 箇所の `unwrap`/`expect` を
  個別に潰す代わりに、単一地点で致命化を防ぐ方針を採った
- [x] 上記に伴い、panic 後の Mutex 汚染で「捕捉したのに以降ずっと壊れる」状態を避けるため、
  `mod.rs` の 13 箇所と `web.rs` のページキャッシュを poisoning 復帰型のロック
  （`lock_or_recover` / `lock_cache`）に置換

---

### Phase 20 — ハンドラのカテゴリ別分割 v3.4+

`src/server/mod.rs` に 91 個の `#[tool]` ハンドラが同居し 2,831 行に膨れていた。
トークン削減を掲げるプロダクト自身の最大ファイルという点で、可読性だけでなく
ドッグフーディング上の問題でもあった。

- [x] `src/server/handlers/` を新設し、`help()` のカテゴリと 1:1 で対応する 15 モジュール
  （file 20 / write 13 / schema 12 / analysis 10 / git 6 / text 5 / task 5 / memory 4 /
  web 3 / test 3 / session 3 / notebook 2 / log 2 / debug 2 / cmd 1）へハンドラを移設。
  カテゴリ対応は help.rs のカタログから機械的に導出し、重複・欠落ゼロを確認済み
- [x] 各モジュールに `#[tool_router(router = <cat>_router, vis = "pub(crate)")]` を付け、
  `handlers::tool_router()` で全カテゴリをマージ（rmcp 0.3.2 の `ToolRouter` の
  `std::ops::Add` を利用）。ケイパビリティゲートはマージ後に `T0k3nServer::new` で適用
- [x] ハンドラ側は `use crate::server::*;` の 1 行で済むよう、mod.rs の `use` を
  `pub(crate) use` の再エクスポートに変更。ヘルパー（`err`/`ok_json`/`ok_text`/
  `delta_key`/`lock_or_recover` 等）と `EffectiveRoot`・構造体フィールドを `pub(crate)` 化
- [x] `instrument!` は `macro_rules!` の字句スコープに依存するため、`mod handlers;` の宣言を
  マクロ定義より後ろに置く必要がある。その制約をコメントで明示
- [x] 登録漏れ検出テスト `merged_router_registers_exactly_the_declared_tools` を追加。
  全ケイパビリティ有効の状態でマージ後ルーターと `REGISTERED_TOOLS` の集合一致を検証する
  （カテゴリモジュールを追加してマージし忘れると黙ってツールが消えるため）
- [x] 結果: mod.rs 2,831 行 → 1,091 行。最大のハンドラファイルは file.rs 435 行。
  実機確認: 既定 79 / 全ケイパビリティ 91 / `--tools git,debug` 8 / `--disable-commands` で
  run_command 消失

---

### Phase 21 — documents フィーチャー（重量パーサーの切り離し） v3.4+

`pdf-extract` 0.7 と `docx-rs` 0.4 はバイナリサイズとパーサー攻撃面の両方で重い。
`convert_document` 専用の依存なので、フィーチャーで切り離せるようにした。

- [x] `[features] default = ["documents"]` / `documents = ["dep:pdf-extract", "dep:docx-rs"]`。
  `--no-default-features` で両依存がツリーから完全に消える（`cargo tree` で 2→0 を確認）
- [x] `#[tool_router]` は渡されたトークンからルート一覧を組み立てるため、
  個々のハンドラに `#[cfg]` を付けてもルート登録は残ってしまう（cfg 除去はマクロ展開後）。
  そのため `convert_document` を `handlers/document.rs` として独立モジュール化し、
  モジュールとルーターごと `#[cfg(feature = "documents")]` で落とす方式にした。
  `help()` のカテゴリ上は引き続き `web` に属する
- [x] `tool_availability()` / `unavailable_tools()` を追加。`REGISTERED_TOOLS` は
  「宣言されたカタログ」でありライブルーターではない（書き込み系や診断もゲート次第で
  登録されない）ため、`--list-tools` が 91 件を無条件に「registered」と表示するのは
  不正確だった。各行にゲート理由・ビルド不在理由を注記し、ヘッダに
  「91 tools, N available in this build」を表示する
- [x] `debug_info` に `compiled_out_tools` を追加（実行時ゲートとコンパイル時除外の区別）
- [x] ルーター整合テストを slim ビルド対応に（`DOCUMENT_TOOLS` を差し引く）
- [x] CI に `cargo test --no-default-features` と
  `cargo clippy --no-default-features -D warnings` を追加（ubuntu のみ）。
  両構成でテスト 197 件緑・警告 0

---

## 6. 決定事項

| # | 内容 | 決定 |
|---|---|---|
| 1 | パッケージ名・バイナリ名 | パッケージ名 **`t0k3n-mcp`**・バイナリ名 **`t0k3n`**（v2.6.0 で `t0k3n-mcp` から改名。タイプしやすさ優先） |
| 2 | tree-sitter パーサーの追加方式 | **Cargo クレートとしてビルド時に静的バンドル**（実行時 DL なし）。新言語対応は新リリースで提供 |