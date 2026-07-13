# Testing Strategy

This document describes the test layers, fixture corpus, and performance metrics that pin
`latexd` behavior.

## Current Local Oracle Harness

`latexd` has an ignored integration test for local arXiv source/PDF corpora:
[`crates/latexd/tests/arxiv_oracle.rs`](../crates/latexd/tests/arxiv_oracle.rs).
The repository only stores a small manifest in
[`fixtures/arxiv-oracle/cc0-smoke.json`](../fixtures/arxiv-oracle/cc0-smoke.json);
it does not vendor full paper sources or PDFs.

Fetch the configured corpus outside the repository:

```bash
python3 scripts/fetch_arxiv_cc0_corpus.py --output /tmp/latexd-arxiv-cc0
```

Run the oracle test:

```bash
LATEXD_ARXIV_CC0_CORPUS=/tmp/latexd-arxiv-cc0 \
  cargo test -p latexd --test arxiv_oracle -- --ignored --nocapture
```

The vendored multi-revision arXiv smoke corpus is also intentionally excluded
from default CI because production whole/per-page rendering makes the full
6,357-file, 36 MiB sweep an approximately 18-minute test. Run it explicitly for
manual or nightly verification:

```bash
cargo test -p latexd --test arxiv_smoke -- --ignored --nocapture
```

The oracle compares build success, diagnostics, extracted text token counts, raw
and normalized unique-token overlap, page count, and first-page raster gross
status. Normalized overlap folds common Greek symbols, ligatures, and soft
hyphens before token comparison so harmless PDF text extraction differences are
visible separately from real missing text. The report also includes
`metric_findings` buckets for build failure, low internal text count, low raw or
normalized overlap, normalization-sensitive overlap, page-count drift, and
first-page raster gross failures. Successful internal builds also copy the
render-IR `document-ir.json` artifact into the report directory and emit
`ir_structure_slices` for `front_matter`, `abstract`, `body`, `caption`,
`table`, `references`, and `fallback` text where the IR can identify those
regions. Each slice records raw and normalized overlap against the official PDF
text so missing front matter, captions, references, and fallback leakage can be
triaged without manually diffing the full text artifact. Slice reports also
record how many internal-only extra tokens are backed by the corresponding IR
source spans. This distinguishes likely `pdftotext` extraction blind spots, such
as dense table cells, from internal renderer hallucinations; it is diagnostic
metadata, not a strict pass/fail gate. Successful builds also copy the
render-IR `display-list.pdf` artifact and its extracted text into the report
directory, rasterize its first page, and populate a nested `display_list_render`
object with raw/normalized overlap, page-count tolerance, raster gross status,
and raster diff metrics. The internal production PDF now uses that same
`PageDisplayList` rendering path, so strict mode gates production output. The
nested debug-copy measurements remain useful for artifact consistency and
diagnosis while line breaking and page density are still being calibrated. Each
CC0 case can configure
`max_page_count_delta` and `min_first_page_ink_ratio`; the report records
`page_count_within_tolerance` and `first_page_raster_gross` so Phase 2 page-count
and missing-major-text-block regressions are visible without manual artifact
inspection. Use `LATEXD_ARXIV_ORACLE_STRICT=1` to turn the configured thresholds
into hard failures.

## 테스트 전략

### 테스트 레이어

#### A. micro tests

엔진 primitive 검증용.

* tokenizer
* macro expansion
* grouping
* registers
* box/glue
* page builder
* aux IR

#### B. compat fixtures

손으로 관리하는 작은 실제 문서.

* article
* amsmath
* bibliography
* figures
* local style
* hyperref

#### C. arXiv paired corpus

실전 회귀용.

arXiv는 metadata를 OAI-PMH/API로 제공하고, full-text PDF와 source files를 S3로 제공한다. full-text를 기반으로 도구를 만들 때는 다운로드 링크를 arXiv로 돌려야 하며, 대량 수집은 main site가 아니라 S3 / export 경로를 쓰는 것이 맞다. ([arXiv][13])

권장 디렉터리:

```text
corpus/
  micro/
  compat/
  arxiv-smoke/
  arxiv-golden/
  arxiv-nightly/
  known-issues/
  mutations/
  perf/
```

### arXiv Corpus 구성

#### `arxiv-smoke` (PR마다)

* 200개 내외
* single-file / multi-file / local style / bib / figures 분산
* optional `revN/` overlays로 same-project multi-revision semantic stability/backdating/rebuild semantics도 함께 검증

#### `arxiv-golden` (nightly)

* 2,000개 내외
* 연도 / 분야 / 크기 / compiler / package stratified sample

#### `arxiv-nightly`

* 10,000개 이상
* 실패율과 성능 회귀 감시

#### `known-issues`

arXiv 문서에서 직접 뽑은 리스크 문서들. 위의 M12 목록 그대로.

### Mutation Corpus

문서를 하나 고른 뒤 자동 patch를 만든다.

* 본문 단어 하나 수정
* 수식 하나 수정
* label 이름 변경
* caption 수정
* section 추가
* 그림 교체(같은 bbox)
* 그림 교체(다른 bbox)
* `.bbl` 항목 수정
* preamble package option 변경

이 corpus의 목적은 **정답 PDF 비교**가 아니라 **무효화 범위와 HMR latency 측정**이다.

### Oracle Layers

오라클은 세 겹으로 둔다.

1. **system `pdflatex` / TeX Live 2025**
2. **TeX Live 2023**
3. **Tectonic** (보조 오라클)

arXiv 목표가 분명하므로 2025/2023 프로파일이 최우선이다. Tectonic은 2차 sanity check다. ([arXiv][1])

### 비교 기준

PDF 바이트 비교는 하지 않는다.

1. compile success/fail
2. page count exact
3. page raster diff
4. extracted text diff
5. labels / toc / citations semantic diff
6. logs / diagnostics category diff

---

## TDD 계획

### 가장 먼저 TDD할 컴포넌트

#### `hmr-protocol`

* serde roundtrip
* reducer state transitions
* stale revision ignore

#### `tex-world`

* path normalization
* `00README`
* root compile semantics

#### `tex-lexer`

* golden token fixtures
* property tests
* fuzz

#### `tex-vm`

* transcript tests
* scope tests
* regression fixtures

#### `tex-aux`

* parse/write roundtrip
* semantic equality
* backdating

#### `tex-checkpoint`

* snapshot restore
* delta apply
* hash stability

#### `page diff`

* unchanged page id 유지
* insert/delete/replace correctness

### Contract/Smoke로 갈 컴포넌트

#### `tex-render-gs`

* `MockRenderer`로 contract 먼저
* real GS integration은 optional CI job
* golden pixel diff는 허용 오차 기반

#### 전체 성능

* criterion/bench harness
* TDD보다 benchmark-first

### 테스트 도구 추천

* Rust unit tests
* property tests
* fuzzing
* Playwright로 브라우저 E2E
* golden PNG diff
* reduced failure artifact 업로드

---

## 성능 지표

매 빌드마다 이 숫자를 남긴다.

* cold build time
* warm no-op build time
* edit build time
* dirty files
* start checkpoint index
* rebuilt pages
* reused pages
* rerun count
* rendered tiles count
* preview first-paint latency
* peak RSS
* cache size

가장 중요한 KPI는 세 개다.

1. **warm no-op**
2. **late-body edit**
3. **tile patch latency**

---
