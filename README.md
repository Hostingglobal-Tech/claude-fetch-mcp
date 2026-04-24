# claude-fetch-mcp

**Claude Code 내장 `WebFetch` 를 대체하는 Rust MCP 서버. 단순 URL 조회에서 60 배 빠름.**

HTTP fetch + HTML→markdown 변환 + SQLite TTL 캐시 를 Rust 네이티브로 수행. LLM 요약 단계를 생략해 Claude 본인이 markdown 을 직접 읽게 하는 구조. 페이지 크기가 작거나 반복 fetch 할 때 체감 속도 차이가 큼.

## 벤치마크

10 URL × 5 runs (Windows 10, 일반 가정용 인터넷 회선).

| 스택 | 평균 cold | 대비 |
|---|---:|---:|
| **claude-fetch-mcp** (Rust) | **147 ms** | 1× (기준) |
| Python (`requests` + `BeautifulSoup` + `html2text`) | 1,210 ms | 8.2× 느림 |
| Claude Code 내장 `WebFetch` (LLM 요약 포함) | ~9,000 ms | **~60× 느림** |

캐시 hit 시 Rust MCP 는 평균 **37 ms** (warm). 자세한 측정 방법·URL 별 수치·한계는 [`BENCHMARK_REPORT.md`](./BENCHMARK_REPORT.md).

## 왜 빠른가

1. **LLM 요약 생략** — 내장 `WebFetch` 는 fetch 후 Haiku 급 모델로 요약해서 반환. 이 단계가 전체의 80~90%. 본 도구는 markdown 만 반환하고 Claude 본인이 필요한 만큼 읽음.
2. **Rust 네이티브** — `reqwest` + `rustls` + `http2` + `pool-keepalive`. TLS 는 AES-NI 하드웨어 가속.
3. **영속 프로세스** — MCP stdio 는 쉘 세션 동안 프로세스 유지. TLS 핸드셰이크·SQLite open 1 회만 수행.
4. **SQLite TTL 캐시** — 반복 fetch 시 디스크 read 만 (대역폭·네트워크 스킵).

**어셈블리 최적화 아님**. 진짜 병목은 I/O 도 파싱도 아니라 LLM 요약 레이어였음.

## 설치

### 1. 사전 준비
Claude Code CLI 설치되어 있고 `claude mcp` 명령 사용 가능해야 함.

### 2. 바이너리 다운로드

미리 빌드된 바이너리는 [Releases](https://github.com/Hostingglobal-Tech/claude-fetch-mcp/releases/latest) 에서 OS 별로 받을 수 있음.

**Linux / macOS (bash)**:
```bash
curl -fsSL https://raw.githubusercontent.com/Hostingglobal-Tech/claude-fetch-mcp/main/install.sh | bash
```

**Windows (PowerShell)**:
```powershell
iwr -useb https://raw.githubusercontent.com/Hostingglobal-Tech/claude-fetch-mcp/main/install.ps1 | iex
```

### 3. 소스 빌드 (선택)
```bash
git clone https://github.com/Hostingglobal-Tech/claude-fetch-mcp.git
cd claude-fetch-mcp
cargo build --release
# 결과: target/release/claude-fetch-mcp(.exe)
```

필요 Rust 버전: **1.75+** (edition 2021).

### 4. Claude Code 에 등록

```bash
claude mcp add --scope user fetch-rs -- /full/path/to/claude-fetch-mcp
# 확인
claude mcp list
# fetch-rs: /full/path/to/claude-fetch-mcp  - ✓ Connected
```

## 제공 도구

| 도구 | 용도 |
|---|---|
| `fetch_url` | HTTP 메타 + 짧은 스니펫 + title. 페이지 존재 확인 / 간단 조회 |
| `fetch_markdown` | HTML → markdown 전체 변환 결과. Claude 가 본문을 읽을 때 |
| `fetch_raw` | raw body (JSON / plain text / 크롤러 피드) |
| `cache_stats` | 캐시 hit/miss/entries 통계 |
| `cache_purge` | TTL 지난 엔트리 삭제 |

공통 인자:
- `url` (필수) — http / https
- `ttl_secs` (기본 3600) — 캐시 유효 시간
- `bypass_cache` (기본 false) — 강제 재fetch
- `max_bytes` — 반환 본문 크기 상한 (기본 1 MB)

## 언제 쓰고 언제 안 쓰나

**추천**:
- docs / API 문서 / README / RFC 같이 정적이고 작은 페이지 (< 50 KB)
- 반복 조회 (같은 URL 여러 번) — 캐시 hit
- JSON / plain text 피드
- 정확한 원문이 필요할 때 (LLM 요약이 왜곡 가능한 경우)

**비추 / 내장 `WebFetch` 유지**:
- 50 KB+ 큰 HTML (요약이 context token 아끼는 데 유리)
- JS 렌더링 필수 페이지 (SPA)
- 로그인 / 쿠키 필요 (Confluence, Notion 등 — 전용 MCP 사용)

두 도구는 **보완 관계**. Claude 가 판단해서 자동 선택하게 두면 됨.

## 설정

### 환경 변수

| 변수 | 기본값 | 설명 |
|---|---|---|
| `CLAUDE_FETCH_CACHE` | `%LOCALAPPDATA%\claude-fetch-mcp\cache.sqlite` (Windows) / `~/.cache/claude-fetch-mcp/cache.sqlite` (Linux/macOS) | SQLite 캐시 파일 경로 |
| `CLAUDE_FETCH_LOG` | `info` | tracing 로그 레벨 (`trace`/`debug`/`info`/`warn`/`error`) |

### 구조

```
Claude Code
  │ MCP JSON-RPC stdio
  ▼
claude-fetch-mcp (단일 바이너리, 약 8.5 MB)
  ├─ reqwest + rustls-tls (HTTP/2, gzip, brotli, keep-alive pool)
  ├─ html2md (HTML → markdown)
  ├─ rusqlite (SQLite WAL, SHA256 key, TTL)
  └─ 노출 도구 5 개 (위 표 참조)
```

## 투명성

- **API 키 / 비밀번호 / 인증 토큰: 포함 0**. 소스 전수 grep 후 확인.
- **외부 서비스 호출: 없음**. Anthropic / OpenAI / 제3자 API 직접 호출 없음. 순수 HTTP GET 만 수행.
- **텔레메트리: 없음**. 어떤 외부 서버에도 데이터 전송 안 함.
- **캐시 저장 위치: 로컬 디스크만**. 외부 동기화 없음.
- **User-Agent**: `claude-fetch-mcp/<version>` (개인 식별 정보 0).

의존성은 전부 [crates.io](https://crates.io) 공개 crate. 민감 정보 처리 / 네트워크 통신은 `reqwest` + `rustls` 가 담당 (러스트 생태 표준).

## 한계

1. **JS 렌더 페이지 미지원**. headless 브라우저 영역 (별도 도구).
2. **robots.txt / rate limit 미준수**. 상용 사이트 대상으로 자동화 시 주의.
3. **인증 필요 페이지 불가**. Confluence / Notion / Google Docs 등은 전용 MCP 사용.
4. **HTML → markdown 품질**. Wikipedia / Hacker News 같이 DOM 이 깊은 페이지에서는 markdown 이 비대해지는 경향. `readability-rs` 기반 추출로 개선 검토 중.

## 재현

```bash
# 벤치 실행
cd claude-fetch-mcp
cargo build --release
./target/release/fetch-bench --urls bench_urls.txt --runs 5 --out bench_rust.json

# Python baseline 비교 (선택)
pip install requests beautifulsoup4 html2text
python bench_python.py bench_urls.txt 5 bench_python.json
```

입력 URL 리스트는 [`bench_urls.txt`](./bench_urls.txt). 여러분 환경에서 재측정 권장.

## 라이선스

MIT. [`LICENSE`](./LICENSE).

## 기여

이슈 / PR 환영. 언어는 한글 / 영어 아무거나 OK.
