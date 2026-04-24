# 벤치마크 보고서

claude-fetch-mcp v0.1.0 vs 대조군 2종.

## 실행 환경

- OS: Windows 10 Pro (빌드 19045)
- CPU: 일반 사무용 (구체 모델 비공개)
- 회선: 일반 가정용 인터넷
- 측정일: 2026-04-24 (KST)
- Rust: 1.75+
- 빌드 프로파일: `release` (`opt-level = 3`, `lto = "fat"`, `strip = true`)

## 대상

| # | 스택 | 설명 |
|---|---|---|
| A | **claude-fetch-mcp** | Rust 단일 바이너리. `reqwest` + `rustls` + `html2md` + `rusqlite`. LLM 요약 없음. |
| B | Python baseline | `requests` + `BeautifulSoup` + `html2text`. LLM 요약 없음. |
| C | Claude Code 내장 `WebFetch` | Anthropic 공식 도구. HTTP fetch + HTML→markdown + **LLM 요약** |

## 입력 (`bench_urls.txt`)

10 URL, 다양한 유형 혼합:

```
https://doc.rust-lang.org/book/ch01-00-getting-started.html
https://doc.rust-lang.org/std/collections/struct.HashMap.html
https://www.rust-lang.org/
https://example.com/
https://raw.githubusercontent.com/rust-lang/rust/master/README.md
https://en.wikipedia.org/wiki/Rust_(programming_language)
https://news.ycombinator.com/
https://api.github.com/repos/rust-lang/rust
https://docs.claude.com/en/docs/claude-code/overview
https://v2.tauri.app/
```

## 측정 방법

### A — claude-fetch-mcp

동일 바이너리에 번들된 `fetch-bench`:

```bash
./target/release/fetch-bench --urls bench_urls.txt --runs 5 --out bench_rust_mcp.json
```

- `cold` = 매 run 전 SQLite 캐시 비움 (`purge_expired(0)`)
- `warm` = 동일 URL 재호출 → cache hit + markdown 재파싱
- 각 URL 당 5 회 반복

### B — Python baseline

```bash
pip install requests beautifulsoup4 html2text
python bench_python.py bench_urls.txt 5 bench_python.json
```

동일 URL 리스트, 5 회 반복. `requests` 로 HTTP GET, `BeautifulSoup` 파싱, `html2text` 로 markdown 변환. 각 run 측정치 = `fetch_ms + parse_ms` 합계.

### C — 내장 `WebFetch`

Bash `date +%s%3N` 로 호출 전·후 wall-clock. 2 URL 샘플 측정.

## 결과

### 집계

| 스택 | 평균 cold (ms) | 평균 warm (ms) | A 대비 |
|---|---:|---:|---:|
| **A. claude-fetch-mcp** | **146.7** | **37.2** | 1× (기준) |
| B. Python baseline | 1,210.2 | — | **8.2× 느림** |
| C. 내장 `WebFetch` | ~9,000–10,000 | 15 분 cache TTL 내 즉시 | **~60× 느림** |

A 의 캐시 속도 증가: **3.9×** (146.7 → 37.2 ms).

### URL 별 상세 (단위 ms)

| URL | A cold | A warm | B cold | md 바이트 |
|---|---:|---:|---:|---:|
| doc.rust-lang.org/book/ch01 | 24 | 1 | 799 | 2,650 |
| doc.rust-lang.org/std HashMap | 33 | 21 | 1,259 | 111,263 |
| rust-lang.org 홈 | 44 | 1 | 1,493 | 5,501 |
| example.com | 58 | 0 | 790 | 337 |
| github raw README | 61 | 0 | 654 | 3,304 |
| en.wikipedia Rust | 489 | 249 | 2,027 | 19.9 MB (주석 참고) |
| news.ycombinator.com | 290 | 52 | 1,401 | 3.8 MB (주석 참고) |
| api.github.com JSON | 77 | 0 | 774 | 6,179 |
| docs.claude.com overview | 254 | 42 | 1,766 | 973 KB |
| v2.tauri.app | 138 | 7 | 1,139 | 39,703 |

### C 샘플 측정

| URL | WebFetch wall-clock |
|---|---:|
| `https://example.com/` | 9,761 ms |
| `https://doc.rust-lang.org/std/collections/struct.HashMap.html` | 9,093 ms |

페이지 크기와 무관하게 9 초 내외. LLM 요약 단계가 지배적.

## 해석

1. **A vs C ≈ 60×**: 대부분 LLM 요약 생략 효과. 네트워크 / 파싱 자체는 전체의 10% 이하.
2. **A vs B = 8.2×**: 동일 파이프라인 (HTTP + HTML→markdown, LLM 없음). Rust 네이티브 + HTTP/2 pool + AES-NI TLS + 영속 프로세스 캐시 덕.
3. **A cache hit 37 ms**: 대부분 시간은 html2md 재파싱. HTTP I/O 는 0 (SQLite read).

## 알려진 한계

### html2md 비대화 (⚠)

`html2md` crate 는 Wikipedia / Hacker News 처럼 복잡 DOM 에서 markdown 이 원본보다 커짐 (19.9 MB, 3.8 MB). 실제 텍스트보다 attribute/class 가 보존됨. 해결책:

- `readability-rs` 또는 `dom_smoothie` 로 "본문 추출" 선행 후 변환
- `max_bytes` 기본값을 1 MB → 100 KB 로 낮춤

v0.2 로드맵에 포함.

### 벤치마크 자체의 한계

- 네트워크 상태 변동 (각 run 간 ±30 ms 일반적)
- 측정 환경 가정용 (엔터프라이즈 low-latency 환경에서 숫자 작아질 수 있음)
- C (내장 WebFetch) 는 2 URL 샘플만 — Claude API 호출 비용 때문에 전체 10 URL × 5 runs 측정 안 함
- A 의 warm 은 SQLite cache hit 이지만 markdown 재파싱 포함 — 순수 cache 조회만 측정하려면 markdown 변환 결과도 캐시해야

## 재현

모든 입력 / 측정 스크립트 / 결과는 repo 에 포함. 여러분 환경에서 재측정 권장.

```bash
git clone https://github.com/Hostingglobal-Tech/claude-fetch-mcp.git
cd claude-fetch-mcp
cargo build --release
./target/release/fetch-bench --urls bench_urls.txt --runs 5 --out my_result.json
```

## 결론

- 작은 페이지 / 반복 조회에서 내장 `WebFetch` 대비 대폭 빠름
- 큰 페이지는 LLM 요약의 context token 절약 효과가 있어 내장 도구 유지 권장
- 두 도구는 **보완 관계**, 대체 관계 아님
