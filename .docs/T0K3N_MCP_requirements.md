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
- 判別した言語に対応する tree-sitter パーサーを自動ダウンロードする
- ダウンロード済みパーサーはキャッシュし、次回起動時は再利用する

#### 言語判別ロジック

1. ワークスペース直下のファイル拡張子を集計
2. `package.json` / `Cargo.toml` / `go.mod` 等のマニフェストファイルで補完
3. 上位 N 言語のパーサーをダウンロード（上限: 設定可能、デフォルト 5 言語）

#### キャッシュ仕様

- 保存先: `~/.cache/t0k3n-mcp/parsers/<言語名>/<バージョン>/`
- 同一バージョンが存在する場合はスキップ
- キャッシュの有効期限: なし（手動削除 or `--refresh-parsers` フラグで更新）

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
| PDF | `pdf-parse` |
| DOCX | `mammoth` |
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

## 3. 非機能要件

### 3.1 パフォーマンス

- MCP ツール応答: 通常ファイルで **200ms 以内**（tree-sitter パース含む）
- 起動時パーサーダウンロード: バックグラウンド実行、ダウンロード中もツール使用可能
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
~/.cache/t0k3n-mcp/
  parsers/
    tree-sitter-rust/0.21.0/
    tree-sitter-python/0.21.0/
    tree-sitter-typescript/0.21.0/
    ...            ← 検出言語を無制限にダウンロード
```

---

## 4. ツール一覧サマリ

### ファイル読み取り系

| ツール | 種別 | 説明 |
|---|---|---|
| `read_directory_tree` | Rust 実装 | .gitignore 適用済みディレクトリツリー |
| `read_markdown_toc` | Rust 実装 | MD の見出し一覧 |
| `read_markdown_section` | Rust 実装 | anchor 指定でセクション取得 |
| `read_code_skeleton` | Rust 実装 | tree-sitter による AST ベーススケルトン |
| `read_code_body` | Rust 実装 | スケルトン ID 指定で本文取得 |
| `read_git_diff` | Rust 実装 | 圧縮済み git diff |
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

---

## 5. 実装フェーズ

### Phase 1 — Rust MVP

- [x] Cargo プロジェクト初期化・`rmcp` セットアップ
- [x] 起動時言語判別・tree-sitter パーサー自動ダウンロード・キャッシュ（言語判別のみ実装。parser自動DLはPhase 3）
- [x] ファイル読み取り系ツール全実装
- [x] `read_code_skeleton` / `read_code_body`（regex ベース実装、tree-sitter は Phase 2）
- [x] `fetch_webpage`（htmd）+ `read_webpage_section`
- [x] `convert_document`（PDF/DOCX → MD・一時ファイル）
- [x] `compress_text`
- [x] `count_tokens` / `check_budget`
- [x] SQLite（記憶・タスク・セッション）

### Phase 2 — 安定化・最適化

- [ ] ベンチマーク測定
- [x] `--refresh-parsers` フラグ
- [x] エラーハンドリング強化（パストラバーサル防止・入力バリデーション）
- [x] MCP Instructions 整備（ツールの使い方を LLM に伝える）
- [x] バイナリ配布（GitHub Actions release.yml）

### Phase 3 — 拡張（要検討）

- [ ] Deno スクリプト連携（補助用途）
- [ ] 対応言語・フォーマット追加

---

## 6. 決定事項

| # | 内容 | 決定 |
|---|---|---|
| 1 | tree-sitter パーサーの対応言語上限 | **無制限**（検出した言語を全てダウンロード） |
| 2 | パッケージ名・バイナリ名 | **`t0k3n-mcp`** |
| 3 | パーサーキャッシュのディレクトリ構造 | クレート名＋バージョン形式（例: `~/.cache/t0k3n-mcp/parsers/tree-sitter-rust/0.21.0/`） |