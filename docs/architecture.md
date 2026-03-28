# Architecture

This document describes the system shape, design constraints, and crate boundaries for
`latexd`.

## 목표와 비목표

### 목표

이 프로젝트의 1차 목표는 다음이다.

1. **대부분의 arXiv 논문**이 `pdflatex` 경로에서 정상 컴파일된다.
2. 편집 중에는 **전체 PDF 재생성**이 아니라 **변경된 페이지 또는 타일만 갱신**된다.
3. 브라우저 프리뷰는 **스크롤/줌/현재 페이지를 유지**하면서 바뀐 출력만 교체한다.
4. 실패 시에도 **마지막 성공 프리뷰는 유지**하고, 진단 정보만 갱신한다.
5. 구현은 처음부터 “전부 새로”가 아니라, **외부 오라클 컴파일러를 이용한 세로 슬라이스**로 시작해서 점진적으로 Rust 코어로 대체한다.

### 비목표

초기 단계에서 하지 않을 것:

* 100% TeX 호환
* shell-escape
* PSTricks/희귀 드라이버
* 브라우저 내 완전한 PDF 편집기
* 처음부터 `xelatex` 완전 지원
* 토큰 단위의 초미세 incremental

특히 arXiv 자체도 현재는 프로파일과 컴파일 경로를 명시적으로 고정하고 있고, 그림 포맷도 경로별로 다르며, root 기준 경로 해석을 요구한다. 따라서 **호환성 범위를 정하고 들어가는 것**이 매우 중요하다. ([arXiv][1])

---

## 가장 중요한 원칙

### 원칙 1: 먼저 세로 슬라이스를 만든다

처음부터 Rust TeX 엔진을 만들지 말고, **`latexd` 데몬 + 웹 프리뷰 + 외부 컴파일러(Tectonic 또는 system `pdflatex`)**로 end-to-end HMR UX를 먼저 만든다. 이렇게 해야 프로토콜, 브라우저 상태 유지, 실패 처리, diff 아티팩트, 테스트 하네스를 먼저 굳힐 수 있다. Tectonic은 완전한 TeX/LaTeX 엔진으로서 캐시, 안정화 rerun, 의존성 규칙 출력 같은 기능이 있어 **오라클/참조 구현**으로 쓰기 좋다. ([Tectonic][3])

### 원칙 2: 백엔드 HMR과 프론트엔드 HMR을 분리한다

* **백엔드 HMR** = 변경을 받아 빌드를 최소 범위로 재실행하는 것
* **프론트엔드 HMR** = 화면에서 바뀐 페이지/타일만 교체하는 것

둘을 분리해야 한다. Ghostscript는 프론트엔드 HMR의 좋은 가속기지만, TeX의 aux/레이아웃/페이지 나눔 문제를 해결해 주지는 않는다. 반대로 TeX 엔진만 빨라도, 브라우저가 매번 전체 PDF를 다시 열면 UX가 나빠진다. Ghostscript의 rectangle-request는 타일 렌더링에 좋고, Tinymist의 웹 프리뷰는 PDF 프리뷰보다 빠르다고 명시적으로 권장된다. ([Ghostscript][4])

### 원칙 3: page number가 아니라 page id를 쓴다

페이지 번호는 앞쪽에 한 페이지 삽입되면 전부 밀린다. 따라서 뷰어와 캐시는 `1, 2, 3`이 아니라 **안정적인 `page_id`**를 기준으로 움직여야 한다. 문서 순서는 별도 배열로 들고, 페이지 자체는 `page_id`로 동일성 판단을 해야 한다.

### 원칙 4: TeX VM 내부를 억지로 쿼리화하지 않는다

Salsa는 sound incremental recomputation을 위한 훌륭한 도구지만, TeX의 매크로 확장과 catcode는 너무 동적이다. 따라서 Salsa류 쿼리 시스템은 **파일 바이트, 프로파일, 스냅샷 조회, 페이지 맵, 프리뷰 패치 계산** 같은 **상위 단계**에만 쓰고, `\expandafter`나 개별 control sequence 실행을 tracked query로 쪼개지 않는다. Typst가 incremental parser/module/layout cache를 갖는다고 해도, TeX는 런타임 catcode와 부작용이 훨씬 세다. 따라서 **incremental granularity는 checkpoint/page 수준**으로 잡는 것이 맞다. ([Rust Compiler Development Guide][5])

### 원칙 5: TDD는 “순수한 컴포넌트”에 강하게 적용한다

TDD가 잘 먹는 곳:

* `00README` 파서
* 경로 해석기
* tokenizer / catcode scanner
* macro table / scope stack
* aux semantic IR
* HMR 프로토콜 reducer
* page patch 계산기
* cache DB

TDD가 약한 곳:

* Ghostscript FFI
* 실제 PDF 래스터 품질
* 전체 성능 튜닝

후자는 **contract test + smoke test + golden diff**로 가야 한다.

---

## 최종 구조

```text
Editor / File Watcher
        │
        ▼
   latexd (daemon)
        │
        ├── ProjectActor
        │     ├── World / Resolver
        │     ├── Query DB (Salsa-like outer graph)
        │     ├── Rust TeX Engine
        │     ├── Checkpoint Store
        │     ├── Aux Semantic Store
        │     └── PDF Exporter
        │
        ├── RenderActor (Ghostscript, dedicated thread or process)
        │
        └── PreviewServer (WebSocket + HTTP)
                      │
                      ▼
                Web Viewer
                  ├── PDF.js bootstrap mode
                  ├── Custom page viewer
                  └── Tile viewer
```

### 각 프로세스의 책임

#### `ProjectActor`

문서별 장기 생명주기 객체다.

* 파일 변경 수집
* 빌드 큐 관리
* revision 생성
* checkpoint replay
* page diff 계산
* 진단 정보 집계
* 프리뷰 패치 생성

#### `RenderActor`

렌더링 전용 객체다.

* 입력: page PDF fragment 또는 full PDF + page request
* 출력: page bitmap / tile bitmap
* 구현: 초기엔 CLI/process 기반, 나중엔 Ghostscript API 직접 연결

Ghostscript의 `gsapi_*()` 함수는 한 스레드에서만 호출해야 하므로, 렌더러는 **문서당 전용 actor**로 두는 것이 안전하다. banding/clist 기반의 background rendering은 내부 최적화로만 사용한다. ([Ghostscript][4])

#### `PreviewServer`

브라우저와 양방향 통신한다.

* WebSocket으로 HMR 이벤트 전송
* HTTP로 page/tile bitmap 서빙
* source ↔ preview jump 요청 처리

#### `Web Viewer`

세 단계로 진화한다.

1. **M0–M5:** PDF.js 기반 전체 PDF 교체
2. **M6–M8:** custom page viewer, page 교체
3. **M9+:** tile viewer, viewport 기반 타일 교체

PDF.js는 브라우저에서 사용할 수 있는 범용 HTML5 PDF 뷰어 라이브러리라서 **부트스트랩용으로 매우 좋다**. 하지만 최종 HMR은 custom page/tile viewer가 더 낫다. Tinymist도 웹 프리뷰를 PDF 프리뷰보다 빠른 경로로 권장한다. ([GitHub][6])

---

## 핵심 데이터 모델

아래 타입들은 처음부터 고정하는 것이 좋다.

```rust
type DocId = u64;
type RevId = u64;
type FileId = u32;
type CheckpointId = u64;
type PageId = [u8; 32];
type TileId = [u8; 32];

struct SourceSpan {
    file: FileId,
    start_utf8: u32,
    end_utf8: u32,
    rev: RevId,
}

struct PageMeta {
    page_id: PageId,
    index: u32,          // 현재 revision에서의 순서
    width_pt: f32,
    height_pt: f32,
    source_spans: Vec<SourceSpan>,
    anchor_hash: [u8; 32],
    content_hash: [u8; 32],
}

struct CheckpointMeta {
    checkpoint_id: CheckpointId,
    rev: RevId,
    page_index_after: u32,
    vm_state_hash: [u8; 32],
    aux_sem_hash: [u8; 32],
    input_cursor_hash: [u8; 32],
}
```

### `PageId` 설계

`PageId`는 `page_index`가 아니라 다음 조합으로 만든다.

```text
PageId = blake3(page_content_hash || page_size || anchor_hash || shipout_state_hash)
```

이렇게 해야 앞에 페이지가 삽입돼도 **안 바뀐 페이지는 같은 id**를 유지한다.

### `World` 추상화

Typst가 CLI/web 환경 차이를 `World`로 숨기듯이, 너도 파일/폰트/프로파일/캐시를 `World`로 감싸야 한다. 이 추상화는 나중에 CLI, LSP, 웹 서버, 테스트 하네스가 같은 코어를 공유하게 만든다. ([GitHub][7])

```rust
trait World {
    fn read_source(&self, path: &NormPath) -> anyhow::Result<Arc<[u8]>>;
    fn read_generated(&self, kind: GeneratedKind) -> anyhow::Result<Option<Arc<[u8]>>>;
    fn file_exists(&self, path: &NormPath) -> bool;
    fn profile(&self) -> &CompilerProfile;
    fn project_root(&self) -> &NormPath;
    fn font_catalog(&self) -> &FontCatalog;
}
```

### `CompilerProfile`

```rust
enum TexLiveVersion { TL2023, TL2025 }

enum CompilerMode {
    PdfLatex,
    LatexDvipsPs2Pdf,
    XeLatex,
}

struct CompilerProfile {
    texlive: TexLiveVersion,
    compiler: CompilerMode,
    compile_from_root: bool,
    allow_shell_escape: bool,
    supported_image_exts: Vec<&'static str>,
    bundle_id: String,
}
```

arXiv 프로파일에 맞추면 초기값은 이렇게 된다.

* `compile_from_root = true`
* `allow_shell_escape = false`
* `PdfLatex`에서는 `pdf/png/jpg`
* `LatexDvipsPs2Pdf`에서는 `ps/eps`
* `TL2025` default, `TL2023` optional

이건 arXiv의 현재 제출 시스템 동작과 직접 맞닿아 있다. ([arXiv][1])

---

## 바깥쪽 Incremental Query Graph

상위 레벨은 Salsa 같은 query DB를 쓰는 것이 좋다. 다만 쿼리 granularity는 거칠게 유지한다. ([Rust Compiler Development Guide][5])

권장 쿼리:

```text
source_text(file_id) -> Rope
source_hash(file_id) -> Hash
project_manifest(doc_id) -> ProjectManifest
compiler_profile(doc_id) -> CompilerProfile
resolved_toplevels(doc_id) -> Vec<FileId>
format_key(doc_id) -> Hash
format_snapshot(format_key) -> SnapshotId
dependency_trace(prev_rev) -> DepTrace
dirty_files(prev_rev, current_inputs) -> Vec<FileId>
start_checkpoint(prev_rev, dirty_files) -> CheckpointId
build_result(doc_id, start_checkpoint, current_inputs) -> BuildResult
page_map(rev_id) -> Vec<PageMeta>
preview_patch(prev_rev, new_rev, viewport) -> PatchSet
```

### 여기서 중요한 점

* `source_text`나 `project_manifest`는 query로 캐시
* **TeX VM 내부 step은 query로 만들지 않음**
* `build_result`가 내부에서 VM을 돌리고, 결과물만 query로 노출

---

## 백엔드 HMR 설계

### 빌드 단계

백엔드 빌드는 네 단계다.

1. **변경 수집**
2. **시작 checkpoint 결정**
3. **TeX 재실행**
4. **page diff / patch 생성**

### Checkpoint 전략

초기 checkpoint는 두 종류만 둔다.

* `CP0`: preamble 끝
* `CPn`: 각 `\shipout` 직후

이것만으로도 꽤 쓸 만하다. 이후 최적화로 다음을 추가한다.

* `\include` enter/exit
* bibliography 직전
* common sectioning command trace point (`\section`, `\chapter` 등)

처음부터 구조 체크포인트를 넣으려 하지 말고, **preamble + shipout**만 먼저 구현하라.

### Preamble Snapshot

가장 큰 승부처다. Tectonic도 format file과 support file 캐시에서 큰 이득을 얻는다. 너도 같은 식으로 **preamble 이후 VM 상태를 snapshot**해야 한다. ([Tectonic][2])

키는 다음으로 만든다.

```text
FormatKey =
  hash(
    compiler_profile,
    documentclass + options,
    package list + options,
    local .sty/.cls/.cfg/.def hashes,
    bundle_id,
    preamble semantic state
  )
```

주의할 점: **입력 텍스트만** 해시하지 말고, preamble이 만들어 낸 **상태**를 반영해야 한다. catcode, macro def, font state, page geometry, driver 관련 상태까지 포함해야 한다.

### Semantic Aux

`.aux`를 바이트로 비교하지 말고 **의미 IR**로 비교한다.

```rust
enum AuxItem {
    Label { key: String, value: String, page: String, anchor: String },
    TocEntry { level: u8, title_tex: Vec<Token>, page: String },
    CitationUse { keys: Vec<String> },
    BibData { files: Vec<String> },
    BibStyle { style: String },
}
```

매 빌드 흐름:

1. 이전 concrete `.aux` 읽기
2. semantic IR로 파싱
3. 현재 빌드에서 semantic IR 생성
4. semantic equality 비교
5. 같으면 backdate, 다르면 rerun 필요 표시

이 구조가 있어야 label/page ref 변동 때문에 불필요한 전체 무효화를 피할 수 있다.

### Invalidation Rules

처음엔 단순하게 간다.

* `.cls`, `.sty`, `.cfg`, preamble 변경 → **full rebuild**
* body `.tex` 변경 → **최근접 checkpoint부터**
* 그림 변경 → **그 그림을 처음 소비한 checkpoint부터**
* `.bbl` 변경 → **첫 인용 checkpoint부터**
* `00README`/profile 변경 → **full rebuild**

### Cancellation

실시간 편집에서는 cancel이 중요하다.

규칙:

* 빌드 중 새 edit가 오면 `stale` 플래그 설정
* VM이 safe point(checkpoint 경계 또는 input file boundary)에 오면 중단
* 중단된 빌드의 결과는 patch로 내보내지 않음
* 마지막 성공 프리뷰는 유지
* diagnostics만 갱신

---

## 프론트엔드 HMR 설계

### 단계별 Viewer 전략

#### 단계 A: PDF.js bootstrap

처음엔 **PDF.js로 전체 PDF를 로드**한다. 프론트 HMR은 “문서 다시 열기”가 아니라:

* 현재 `scrollTop`, `zoom`, `currentPage`
* 선택된 source span
* 사이드바 상태

를 유지한 채 PDF blob만 교체하는 방식으로 만든다.

PDF.js는 브라우저에서 라이브러리로 쓸 수 있으므로 초기 구현 속도가 매우 빠르다. ([GitHub][6])

#### 단계 B: custom page viewer

다음 단계부터는 page 리스트를 직접 들고 간다.

```ts
type ViewerState = {
  rev: number
  pageOrder: string[]
  pages: Map<string, PageModel>
  zoom: number
  scrollY: number
  visiblePages: string[]
  selectedSource?: SourceSpan
}
```

패치 연산은 네 개만 있어도 충분하다.

* `ReplacePage(page_id, meta, bitmap_url)`
* `InsertPageAfter(after_page_id, new_page_id, meta, bitmap_url)`
* `DeletePage(page_id)`
* `UpdateAnchors(page_id, anchors)`

#### 단계 C: tile viewer

페이지를 타일로 쪼갠다.

```text
tile_key = (page_id, zoom_bucket, tile_x, tile_y)
```

브라우저는 viewport에 필요한 타일만 요청하고, 서버는 바뀐 타일만 푸시한다.

### Ghostscript Adapter

Ghostscript display device는 `display_update`와 `display_rectangle_request`를 제공한다. rectangle request mode에서는 내부 display list를 만든 뒤 rectangle을 반복 렌더할 수 있어 타일 렌더링에 맞다. 다만 투명도를 쓰는 경우 `display_update`가 전혀 호출되지 않을 수 있으므로, **항상 explicit rectangle request 경로를 주 경로로** 삼아라. `display_update`는 있으면 쓰는 최적화 정도로만 취급해야 한다. ([Ghostscript][4])

렌더러 설계는 이렇게 한다.

```rust
trait Renderer {
    fn render_full_page(&mut self, page: &PageRenderInput, scale: f32) -> RgbaImage;
    fn render_tiles(&mut self, page: &PageRenderInput, scale: f32, rects: &[Rect]) -> Vec<TileImage>;
}
```

실장 순서:

1. `MockRenderer`
2. `CliRenderer` (외부 Ghostscript 프로세스)
3. `GsApiRenderer` (직접 FFI)
4. `GsApiTileRenderer` (rectangle request)

### Source ↔ Preview Jump

Tinymist가 web preview에서 bidirectional navigation을 강하게 밀고 있고, Typst도 tracing을 통해 IDE 기능을 구현한다. 너도 SyncTeX와 비슷한 `SyncMap`을 만들어야 한다. ([Myriad Dreamin][8])

구조:

```rust
struct PageSyncMap {
    page_id: PageId,
    boxes: Vec<SyncBox>,   // bbox + source spans
    anchors: Vec<Anchor>,  // ref, label, link
}
```

이 맵은:

* 소스 커서 이동 → 프리뷰 highlight
* 프리뷰 클릭 → 소스 위치 점프
* dirty source span → 예상 dirty page 계산

에 동시에 쓴다.

---

## Rust Workspace 구조

```text
latex-hmr/
  Cargo.toml
  crates/
    hmr-protocol/
    latexd/
    tex-world/
    tex-archive/
    tex-profile/
    tex-lexer/
    tex-tokens/
    tex-vm/
    tex-trace/
    tex-aux/
    tex-layout/
    tex-pdf/
    tex-checkpoint/
    tex-render-gs/
    preview-server/
    corpus-harness/
  web/
    viewer/
  fixtures/
    micro/
    compat/
    known-issues/
```

### 크레이트 책임

#### `hmr-protocol`

* 서버/클라이언트 메시지 타입
* serde roundtrip
* reducer 입력 이벤트

#### `latexd`

* 문서 actor
* 파일 변경 큐
* build scheduling
* cancellation
* metrics

#### `tex-world`

* project root
* `00README`
* profile
* file resolver
* font catalog
* generated outputs path

#### `tex-archive`

* `.zip`, `.tar`, `.tar.gz`
* normalize/unpack
* root safety

#### `tex-lexer`

* catcode-aware scanner
* control sequence lexing
* line ending normalization
* source span tracking

#### `tex-tokens`

* token arena
* control sequence interner
* token list refs

#### `tex-vm`

* expansion
* grouping
* registers
* conditionals
* macros
* file I/O primitives

#### `tex-trace`

* file reads
* macro/state mutations
* aux reads/writes
* shipout trace
* source→page attribution

#### `tex-aux`

* semantic aux IR
* parser/writer
* equality / backdating

#### `tex-layout`

* h/v list
* glue/penalty
* line breaking
* page builder
* floats/marks 최소 구현

#### `tex-pdf`

* page → PDF
* links/outlines
* image embedding
* font subset

#### `tex-checkpoint`

* VM snapshot
* delta encoding
* restore/replay
* page boundary hash

#### `tex-render-gs`

* Ghostscript process/FFI adapter
* page/tile raster

#### `preview-server`

* WebSocket
* bitmap/tile endpoints
* preview session management

#### `corpus-harness`

* fixture runner
* oracle runner
* raster/text diff
* mutation runner
* reducer

---
