# T0K3N-MCP

> **AI 코딩 도구의 토큰 소비를 최대 87% 줄이는 MCP 서버**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![Token Savings](https://img.shields.io/badge/Token%20Savings-87.3%25-brightgreen)](.docs/benchmark_token_savings.md)

[English](README.en.md) | [日本語](README.ja.md) | [中文](README.zh.md) | **한국어**

---

## 벤치마크: 75–87% 토큰 절감

Anthropic 공식 토큰 카운트 API를 사용하여 **두 개의 실제 프로젝트**에서 측정했습니다.

### 연구 1: Rust 프로젝트 (T0K3N-MCP 자체)

| 파일 | 전체 | T0K3N-MCP | 절감률 |
|------|------|-----------|--------|
| `code.rs` (295줄) | 3,642 | 345 | **90.5%** |
| `mod.rs` (422줄) | 4,997 | 1,162 | **76.7%** |
| `README.md` | 2,492 | 296 | **88.1%** |
| `Cargo.toml` | 491 | 24 | **95.1%** |
| **평균** | 2,147 | 321 | **87.3%** |

### 연구 2: Next.js 프로젝트 (vercel/commerce)

| 파일 | 전체 | T0K3N-MCP | 절감률 |
|------|------|-----------|--------|
| `components/cart/modal.tsx` | 2,776 | 143 | **94.8%** |
| `app/product/[handle]/page.tsx` | 1,400 | 134 | **90.4%** |
| `lib/shopify/index.ts` | 4,073 | 1,299 | **68.1%** |
| `components/cart/cart-context.tsx` | 1,742 | 488 | **72.0%** |
| **평균 (20개 파일)** | 957 | 198 | **75.5%** |

### 전체 프로젝트 시뮬레이션 (5개 태스크 조사)

| | 표준 | T0K3N-MCP | 절감률 |
|-|------|-----------|--------|
| Next.js 조사 | 19,109 tokens | 2,668 tokens | **86.0%** |

> 전체 방법론 및 데이터: [`.docs/benchmark_token_savings.md`](.docs/benchmark_token_savings.md)

200,000 토큰 컨텍스트 창이 실질적으로 **6–8배** 더 커집니다.

---

## 표준 도구가 부족한 이유

Claude Code와 Cursor의 표준 Read File은 전체 파일을 컨텍스트에 덤프합니다:

```
read_file("server/mod.rs")  →  4,997 tokens 소비
                                ↑ 95%는 현재 질문과 무관
```

T0K3N-MCP은 **"구조 먼저, 필요한 부분만 가져오기"** 설계로 이를 해결합니다:

```
read_code_skeleton("server/mod.rs")  →  1,162 tokens (시그니처만)
read_code_body(["function:54-67"])   →    150 tokens (대상 함수만)
                                         ──────────────────────────
합계                                       1,312 tokens  ← 74% 절감
```

---

## 설치

### 사전 빌드된 바이너리 (권장)

GitHub Releases에서 사용 중인 OS의 바이너리를 다운로드하세요.

| OS | 파일 |
|----|------|
| macOS (Apple Silicon) | `t0k3n-mcp-macos-aarch64` |
| macOS (Intel) | `t0k3n-mcp-macos-x86_64` |
| Linux x86_64 | `t0k3n-mcp-linux-x86_64` |
| Linux ARM64 | `t0k3n-mcp-linux-aarch64` |
| Windows x86_64 | `t0k3n-mcp-windows-x86_64.exe` |

### 소스에서 빌드

```bash
git clone https://github.com/tonrakun/t0k3n-mcp
cd t0k3n-mcp
cargo build --release
# → ./target/release/t0k3n-mcp
```

Rust 이외의 의존성 없음. Node.js, npm, Python 불필요.

---

## 설정

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

동일한 설정을 각 클라이언트의 MCP 설정 파일에 추가하면 됩니다.

### 옵션

```
--root <path>          워크스페이스 루트 (필수)
```

---

## 사용 방법

### 코드 파일 (Rust / Python / JS / TS / Go)

```
1. read_code_skeleton("path/to/file.rs")
   → 함수 / struct / impl 시그니처 목록 + ID 반환

2. read_code_body(["function:10-45", "impl:87-130"])
   → 지정된 함수의 본문만 반환
```

### Markdown / 문서

```
1. read_markdown_toc("ARCHITECTURE.md")
   → 앵커가 포함된 제목 목록 반환

2. read_markdown_section("ARCHITECTURE.md", ["#database-design"])
   → 지정된 섹션만 반환
```

### 웹 페이지

```
1. fetch_webpage("https://docs.rs/tokio/latest/tokio/")
   → HTML을 Markdown으로 변환하고 목차 반환

2. read_webpage_section(url, ["#struct-JoinHandle"])
   → 캐시된 Markdown에서 지정된 섹션 반환
```

### PDF / DOCX

```
1. convert_document("report.pdf")
   → Markdown으로 변환하고 목차 + tmp_path 반환

2. read_markdown_section(tmp_path, ["#chapter-3"])
   → 지정된 섹션만 반환
```

### 토큰 예산 관리

```
1. check_budget(budget=8000, candidates=["a.rs", "b.rs", "c.md"])
   → strategy: "full" | "skeleton_only" | "toc_only" | "skip"

2. 권장 전략에 따라 도구 선택
```

---

## 도구 참조 (51개 도구)

### 파일 읽기

| 도구 | 설명 |
|------|------|
| `read_directory_tree` | `.gitignore` 필터링이 적용된 디렉토리 트리 |
| `read_markdown_toc` | Markdown 제목 목록 (TOC) |
| `read_markdown_section` | 앵커로 섹션 본문 가져오기 |
| `read_code_skeleton` | 함수 / 클래스 시그니처만 반환 |
| `read_code_body` | skeleton ID로 함수 본문 가져오기 |
| `read_type_skeleton` | 타입 정의 스켈레톤 (TS interface/type/enum, Go struct/interface, Rust struct/enum/trait) |
| `read_call_graph` | 함수 호출 그래프 — 단일 파일 내 호출자 / 피호출자 |
| `read_token_map` | 워크스페이스 파일 토큰 수 맵 (glob 필터, 내림차순 정렬) |
| `read_symbol_usages` | 워크스페이스 전체에서 심볼 사용 위치 검색 |
| `read_code_deps` | import / imported_by 의존 그래프 |
| `read_file_outline` | 파일 종류 자동 감지 통합 아웃라인 |
| `search_file` | 키워드 / 정규식 매칭 및 주변 컨텍스트 |
| `semantic_search` | 자연어로 의미적으로 관련된 함수 검색 |
| `read_json_yaml_keys` | JSON/YAML/TOML 키 구조 나열 |
| `read_json_yaml_value` | 점 표기법 키 경로로 값 가져오기（JSON/YAML/TOML） |
| `read_openapi` | OpenAPI/Swagger 스펙을 간결한 엔드포인트 목록으로 파싱 |
| `read_env_schema` | .env.example / docker-compose.yml 에서 환경 변수 정의 추출 |

### Git

| 도구 | 설명 |
|------|------|
| `read_git_diff` | 압축된 git diff |
| `read_git_log` | 구조화된 커밋 로그 (저자, 날짜, 변경 파일) |
| `read_git_blame_body` | 함수 본문의 줄별 blame (저자 + 날짜) |
| `read_changed_files` | 브랜치 간 변경 파일 목록 (상태, 추가/삭제 줄 수) |

### DB 스키마

| 도구 | 설명 |
|------|------|
| `read_db_schema` | Prisma / SQL 스키마의 테이블 / 모델 목록 (자동 감지) |
| `read_db_table` | 특정 테이블 또는 모델의 필드 정의 상세 |

### CSS

| 도구 | 설명 |
|------|------|
| `read_css_skeleton` | CSS/SCSS 선택자 목록 (속성 수, 줄 범위) |
| `read_css_body` | 선택자 ID로 규칙셋 본문 가져오기 |

### GraphQL

| 도구 | 설명 |
|------|------|
| `read_graphql_schema` | GraphQL 스키마의 타입 목록 (type/input/enum/interface) |
| `read_graphql_type` | 특정 타입의 필드 정의 상세 |

### 테스트

| 도구 | 설명 |
|------|------|
| `read_test_skeleton` | 테스트 파일의 스위트 / 케이스 목록 (Jest/pytest/Cargo/Go/JUnit/RSpec) |
| `read_test_results` | 테스트 결과 텍스트 파싱 및 요약 반환 (프레임워크 자동 감지) |

### 웹 & 문서

| 도구 | 설명 |
|------|------|
| `fetch_webpage` | HTML → Markdown 변환 + 압축 → TOC |
| `read_webpage_section` | 캐시된 웹 페이지에서 섹션 가져오기 |
| `convert_document` | PDF / DOCX → Markdown 변환 |

### 텍스트 & 예산

| 도구 | 설명 |
|------|------|
| `compress_text` | Markdown 노이즈 및 불필요한 공백 제거 |
| `count_tokens` | 토큰 / 문자 / 줄 수 카운트 |
| `check_budget` | 남은 예산 및 권장 읽기 전략 반환 |
| `summarize_conversation` | 토큰 예산 내 대화 기록 요약 |

### 메모리 / 태스크 / 세션

| 도구 | 설명 |
|------|------|
| `memory_save/get/list/delete` | SQLite 기반 영구 키-값 저장소 |
| `task_create/update/get/list/delete` | 태스크 관리 (상태, 우선순위, 태그) |
| `session_snapshot/restore/list` | 작업 상태 저장 및 복원 |

---

## 지원 언어

`read_code_skeleton` / `read_code_body` / `read_code_deps` 가 지원하는 코드 분석 언어：

| 언어 | 확장자 |
|------|--------|
| Rust | `.rs` |
| Python | `.py` |
| JavaScript | `.js`, `.jsx` |
| TypeScript | `.ts`, `.tsx` |
| Go | `.go` |
| C++ | `.cpp`, `.cc`, `.cxx`, `.hpp` |
| Java | `.java` |
| Ruby | `.rb` |
| C# | `.cs` |
| PHP | `.php` |

파서는 Cargo 크레이트로 빌드 시 바이너리에 정적으로 번들됩니다 — 런타임 다운로드 불필요. 새 언어 지원은 새 릴리스로 제공됩니다. [GitHub Issues](https://github.com/tonrakun/t0k3n-mcp/issues) 에서 요청하세요.

---

## 보안

- `--root` 외부의 모든 경로 해석 차단 (경로 탐색 방어)
- 루트 외부로의 심볼릭 링크 탈출 차단
- 웹 도구(`fetch_webpage`)만 루트 외부 URL 접근 가능 (설계상)

---

## 데이터 저장

```
<root>/.t0k3n/
  t0k3n.db        ← SQLite (메모리, 태스크, 세션)
```

`.gitignore`에 추가 권장:

```gitignore
.t0k3n/
```

---

## 라이선스

[MIT](LICENSE) © 2025 Tonrakun
