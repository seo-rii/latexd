# Implementation Roadmap

This document tracks the milestone order for `latexd` and records what each stage is meant to
prove.

## 구현 순서

여기서부터가 핵심이다. **아래 순서를 바꾸지 않는 것**을 권한다.

---

### M0. 세로 슬라이스: 외부 컴파일러 + 웹 프리뷰

**목표:** HMR UX를 먼저 확인한다.

#### 먼저 쓸 것

* `hmr-protocol`
* `preview-server`
* `web/packages/viewer-core`
* `web/apps/viewer`
* `latexd`
* 외부 컴파일러 adapter (`tectonic` 또는 system `pdflatex`)

#### TDD로 먼저 쓸 테스트

1. 프로토콜 serde roundtrip
2. viewer reducer: `ReplacePage`, `InsertPageAfter`, `DeletePage`
3. 실패 시 last-good preview 유지
4. revision 역전(out-of-order message) 무시
5. scroll/zoom 보존

#### 구현

* 파일 변경 감지
* compile spawn
* 성공 시 PDF blob URL 갱신
* 실패 시 diagnostics 패널만 갱신
* viewer는 PDF.js로 렌더

#### 완료 조건

* `main.tex`를 고치면 브라우저 프리뷰가 자동 갱신된다.
* 현재 페이지와 줌이 유지된다.
* 문법 오류가 나도 이전 프리뷰는 그대로 남는다.

Tectonic은 support files/format cache와 rerun 안정화가 있으므로 M0 오라클로 쓰기 좋다. ([Tectonic][2])

---

### M1. workspace/profile/resolver

**목표:** arXiv-like project model을 확정한다.

현재 구현 상태 메모:

* 이 단계는 현재 저장소 기준으로 이미 구현되어 있다.
* `00README` YAML/JSON 파싱, root 기준 path resolution, toplevel 처리, 이미지 확장자 우선순위, local class/style lookup이 현재 `tex-world`에 들어가 있다.
* 다만 README의 “multi-file arXiv-style fixture 20개”처럼 corpus 규모의 검증은 아직 문서화된 완료 수준까지 넓히지 않았다.

#### TDD 먼저

1. `00README` JSON/YAML parse
2. `texlive_version`/`compiler`/`sources`/`usage=toplevel`
3. compile-from-root path resolution
4. root/subdir toplevel 처리
5. 이미지 확장자 우선순위
6. archive unpack 안전성 (`../` 방지)

#### 구현

* `CompilerProfile`
* `World`
* `ProjectManifest`
* normalize path
* `\input`/`\includegraphics`용 root 기준 resolver
* local style/class lookup

arXiv는 root에서 컴파일하고, `00README`로 프로세스와 소스 사용법을 고정하며, 그림 포맷도 경로별로 다르게 제한한다. ([arXiv][9])

#### 완료 조건

* multi-file arXiv-style fixture 20개를 profile 기반으로 정확히 resolve한다.

---

### M2. 외부 오라클 기반 dependency trace

**목표:** 나중에 내부 엔진이 따라가야 할 dependency ground truth를 먼저 만든다.

현재 구현 상태 메모:

* 이 단계도 현재 저장소 기준으로 구현되어 있다.
* 외부 compiler 경로에서 depfile / recorder 기반 입력 추적, tracked-input hashing, no-op rebuild skip이 들어가 있다.
* revision queue coalescing까지 넓은 의미로 완성된 건 아니고, 현재 핵심은 “변경 없는 warm rebuild를 건너뛴다”와 “dirty input 집합을 저장한다” 쪽이다.

#### TDD 먼저

1. dependency rules parser
2. changed-file → dirty-build plan
3. no-op build short-circuit
4. revision queue coalescing

#### 구현

* Tectonic `--makefile-rules` 또는 로그 기반 의존성 추출
* `DepTrace`
* build cache metadata DB
* changed-file hashing

Tectonic의 compile 인터페이스는 Makefile 형식 의존성 규칙 출력도 제공하므로 초기 trace ground truth로 유용하다. ([Tectonic][10])

#### 완료 조건

* 변경 없는 재실행은 compile spawn 자체를 건너뛴다.
* 바뀐 파일 목록이 정확히 나온다.

---

### M3. tokenizer/interner

**목표:** 순수 Rust TeX 입력 계층을 만든다.

현재 구현 상태 메모:

* 이 단계는 현재 구현되어 있다.
* `tex-tokens`와 `tex-lexer`는 catcode, control sequence interning, comment/space/newline 규칙, UTF-8 span, property-style 회귀를 포함한다.
* 아직 fuzz corpus나 hostile-input hardening을 크게 넓힌 상태는 아니므로, 구현 완료와 robustness 완료는 같은 말이 아니다.

#### TDD 먼저

1. catcode fixture tests
2. control sequence lexing
3. comment/space/newline 규칙
4. source span 보존
5. property tests: line ending 변환, random ASCII, random catcode tables
6. fuzz: broken UTF-8, huge control sequence

#### 구현

* `tex-lexer`
* `tex-tokens`
* token arena
* control sequence interner

#### 완료 조건

* micro fixture에서 expected token stream을 모두 통과한다.

---

### M4. macro VM core

**목표:** expansion과 scope를 구현한다.

현재 구현 상태 메모:

* 이 단계는 starter core를 넘는 수준까지 들어가 있다.
* 현재 VM은 grouping, `\\def`, `\\let`, parameterized macro, `\\expandafter`, `\\csname`, `\\ifdefined`, `\\ifx`, `\\ifnum`, register, minimal `\\input`, transcript, snapshot/restore를 지원한다.
* 반면 완전한 TeX primitive surface나 real LaTeX macro compatibility는 아직 아니다.

#### TDD 먼저

1. `\def`, `\let`, `\expandafter`
2. grouping push/pop
3. register assignment
4. conditionals (`\ifx`, `\ifnum`, `\ifdefined` 정도부터)
5. `\csname...\endcsname`
6. file open/read primitive 최소셋
7. transcript snapshot test

#### 구현

* `tex-vm`
* macro store
* register file
* input stack
* expansion machine

#### 완료 조건

* hand-written mini-TeX fixtures가 transcript 기준으로 맞는다.

---

### M5. mini format → LaTeX kernel bootstrap

**목표:** “내가 만든 VM” 위에서 작은 format을 로드하고, 그 다음 stock LaTeX kernel을 읽는다.

현재 구현 상태 메모:

* 이 단계는 “mini format + local class/package bootstrap” 수준으로 상당 부분 구현되어 있다.
* mini-kernel snapshot, project runner, local `\\documentclass` / `\\usepackage`, `\\newcommand`, missing file / undefined control sequence classification이 현재 코드에 있다.
* 다만 여기서의 “kernel bootstrap”은 실제 stock LaTeX kernel load가 아니라 local `.cls/.sty`와 작은 handcrafted subset에 가깝다.

#### TDD 먼저

1. mini format fixture load
2. LaTeX kernel subset smoke
3. local package load order
4. error classification (`UndefinedControlSequence`, `MissingFile`)

#### 구현

* `format_snapshot`
* kernel loader
* local `.sty/.cls` 로드
* bundle profile

#### 완료 조건

* 아주 작은 article 문서들이 external oracle과 같은 성공/실패 판정을 낸다.

---

### M6. layout + PDF export + internal full build

**목표:** 내부 엔진이 실제 페이지와 PDF를 만든다.

현재 구현 상태 메모:

* 이 단계는 internal full build skeleton까지 구현되어 있다.
* `tex-layout`, `tex-pdf`, internal `latexd --compiler-bin internal`, page metadata, page PDF/SVG artifact emission까지 현재 코드에 들어가 있다.
* 다만 genuine TeX box/glue/page builder는 아직 없고, 현재 layout은 fixed-width text scaffold다. 즉 “internal PDF path exists”와 “real TeX layout engine exists”는 아직 다르다.

#### TDD 먼저

1. hbox/vbox/glue/penalty fixture
2. line break golden tests
3. page builder golden tests
4. PDF page count exact match
5. micro docs raster diff

#### 구현

* `tex-layout`
* `tex-pdf`
* page meta 생성
* basic links / anchors

Typst는 incremental layout cache를 element 단위로 두지만, LaTeX/TeX 쪽에서는 우선 genuine page-building이 먼저다. 테스트도 Typst처럼 parsing/eval/layout/rendering 레이어로 쪼개되, 거기에 **PDF 출력 테스트를 반드시 추가**해야 한다. ([GitHub][7])

#### 완료 조건

* handcrafted compat fixture 50개 이상에서 external oracle과 페이지 수가 일치한다.

---

### M7. preamble snapshot + checkpoint replay

**목표:** 진짜 백엔드 HMR의 시작점.

현재 구현 상태 메모:

* 이 단계는 README의 좁은 milestone 기준으로는 거의 닫힌 상태다.
* preamble snapshot, shipout checkpoint, unchanged tail realignment, nearest checkpoint replay, nested input enter/exit checkpoint, repeated include 보수 처리, warm no-op skip, page patch planning이 현재 코드에 있다.
* 다만 replay selection은 여전히 conservative하며, trace-grade dependency model까지 간 것은 아니다.

#### TDD 먼저

1. snapshot save/load
2. preamble key stability
3. body-only edit 시 CP0 재사용
4. page boundary hash equality
5. unchanged tail realignment

#### 구현

* `tex-checkpoint`
* `CheckpointMeta`
* boundary hash
* replay from nearest checkpoint

#### 완료 조건

* 문서 뒤쪽 edit에서 앞쪽 페이지가 재계산되지 않는다.
* no-op warm build가 현저히 빨라진다.

---

### M8. page-level HMR

**목표:** 전체 PDF가 아니라 **페이지 단위 교체**.

현재 구현 상태 메모:

* 이 단계는 bootstrap을 넘어서 실제 viewer/daemon 동작으로 구현되어 있다.
* viewer는 `pageOrder + pages map` 상태를 유지하고, daemon은 `PatchPages`와 revision-stable page artifacts를 내보내며, unchanged page reuse도 server-driven으로 처리한다.
* 다만 최종 page scene graph renderer는 없고, 현재는 page artifact swapping 중심이다.

#### TDD 먼저

1. `ReplacePage` reducer
2. `InsertPageAfter` reducer
3. `DeletePage` reducer
4. unchanged page id 유지
5. revision race handling

#### 구현

* custom page viewer
* `pageOrder + pages map`
* page bitmap endpoint
* page diff engine

#### 완료 조건

* late-section edit 시 바뀐 페이지들만 교체된다.

---

### M9. Ghostscript tile renderer

**목표:** viewport 기반 타일 HMR.

현재 구현 상태 메모:

* 이 단계는 동작하는 bootstrap으로 구현되어 있다.
* `MockRenderer`, `CliRenderer`, minimal `GsApiRenderer`, runtime pool, page/tile endpoint, memory+disk raster cache, viewport prewarm, concurrent cold-miss dedupe가 현재 코드에 있다.
* 다만 truly persistent session에서 rectangle-request/display-list를 재활용하는 최종 형태는 아직 아니다. cold miss에서는 여전히 setup cost가 남아 있다.

#### TDD 먼저

1. `Renderer` trait contract tests with `MockRenderer`
2. tile key hashing
3. viewport → required tile set 계산
4. stale tile invalidation
5. optional integration test with real Ghostscript

#### 구현

* `tex-render-gs`
* `CliRenderer`
* `GsApiRenderer`
* rectangle request mode

Ghostscript는 rectangle request mode를 통해 내부 display list를 만든 뒤 사각형 단위 렌더를 반복할 수 있다. 다만 API 호출은 한 스레드에서만 해야 하고, 투명도 파일에서는 incremental update callback이 빠질 수 있으므로 actor/process 격리가 좋다. ([Ghostscript][4])

#### 완료 조건

* 확대된 페이지에서 화면에 보이는 타일만 다시 그린다.

---

### M10. syncmap + source/preview jump

**목표:** 프리뷰가 편집 도구가 된다.

현재 구현 상태 메모:

* 현재 저장소에는 coarse syncmap을 넘어서 line-aware sync box, preview click/hover, source jump, URL hash deep-link, in-browser source pane, optional local editor bridge까지 들어가 있다.
* 따라서 이 milestone은 “진짜 layout-engine box geometry”까지는 아니어도, 현재 스캐폴드/웹 UX 기준 완료 조건은 사실상 달성한 상태로 본다.

#### TDD 먼저

1. source span → page box 매핑
2. preview click → nearest span
3. hover highlight roundtrip
4. stale revision syncmap 무시

#### 구현

* `PageSyncMap`
* source marker overlay
* preview click handler
* editor bridge

Tinymist의 cross jump와 partial rendering UX가 좋은 선례다. ([Docs.rs][11])

#### 완료 조건

* 브라우저에서 식을 클릭하면 소스가 해당 줄로 점프한다.

---

### M11. aux semantics + references + `.bbl`

**목표:** semantic aux subsystem이 실제 빌드/리플레이를 지배한다.

**상태:** `2026-03-27` 기준 완료.

세부 단계:

* `M11.1` concrete semantic aux artifact
* `M11.2` semantic equality, backdating, rerun-to-fixpoint
* `M11.3` common natbib/biblatex/theorem/float/reference executed-source rewrite
* `M11.4` checkpoint-seeded replay, observability, unchanged-tail/page reuse

실제 완료 기준:

* `semantic.aux`, `build-meta.json`, `semantic-index.json`이 semantic state를 round-trip/backdate 한다.
* labels, TOC, bibliography가 bounded rerun 안에서 안정화된다.
* semantic-equal rebuild는 prior aux payload, checkpoint, page artifact를 재사용할 수 있다.
* common natbib/biblatex/theorem/float/reference surface가 executed source에 raw command를 남기지 않는다.

현재 구현 상태 메모:

* 이 milestone은 이제 단순 `label/toc/cite` 체크리스트가 아니라, latexd가 실제로 쓰는 semantic aux subsystem 전체를 뜻한다.
* 현재 코드에는 `tex-aux` semantic IR, semantic equality/backdating, rerun-to-fixpoint, aux-sensitive checkpoint reuse, `.bbl` first-class input, file-grouped semantic index, build metadata, raw source와 executed source persistence가 모두 들어가 있다.
* concrete aux artifact도 같이 들어간다. 즉 revision마다 `aux.json`뿐 아니라 concrete `semantic.aux`를 쓰고, 현재 subset에서는 `\newlabel`, `\citation`, `\bibdata`, `\bibstyle`, `\bibcite`, `\@writefile/\contentsline` 같은 aux-style surface와 richer custom records를 함께 round-trip/backdate 한다.
* common natbib/biblatex-style citation surface, theorem/amsthm surface, float/list surface, varioref/cleveref/hyperref common surface는 지금 semantic rewrite 경로에 올라가 있고, executed source와 preview output에서 raw command가 남지 않도록 처리한다.
* 이 기준에서 M11은 “semantic meaning을 concrete artifact로 남기고, 그 semantic state로 rerun/replay/backdating을 구동하는 subsystem”까지를 포함한다.
* 대신 full TeX Live/package compatibility, BibTeX/Biber process emulation, macro-generated semantic discovery, corpus-scale hardening은 M12로 넘긴다.

#### TDD 먼저

1. label parse/write roundtrip
2. concrete `semantic.aux` roundtrip/backdating
3. toc/citation/bibliography semantic equality
4. `.bbl` ingestion + executed-source rewrite
5. rerun until semantic fixpoint
6. checkpoint-seeded aux replay and unchanged-tail reuse

#### 구현

* `tex-aux`
* concrete `semantic.aux`
* semantic equality
* rerun policy
* `.bbl` first-class input
* aux-aware replay + observability

arXiv는 intermediate files를 일반적으로 제거하지만 `.bbl`과 `.ind`는 예외다. 또 `minted` cache와 BibLaTeX/TeX Live 버전 차이는 실제 호환성 이슈다. ([arXiv][12])

#### 완료 조건

* concrete `semantic.aux`가 semantic state를 round-trip/backdate 한다.
* labels, toc, bibliography가 몇 번 안 되는 rerun 내에서 안정화된다.
* semantic-equal rebuild는 prior aux payload/checkpoint/page artifact를 재사용할 수 있다.
* common natbib/biblatex/theorem/float/reference surface가 executed source에 raw command를 남기지 않는다.
* full package/toolchain/corpus hardening은 M12로 넘긴다.
* 현재 구현은 이 completion bar를 충족한 상태로 본다.

---

### M12. arXiv hardening

**목표:** “대부분의 arXiv 논문”에 가까워진다.

**상태:** `2026-03-27` 기준 완료. 남는 항목은 post-`M12` future scope로 분리한다.

상세 backlog는 [`docs/work-backlog.md`](/home/seorii/dev/hancomac/latexd/docs/work-backlog.md) 에, `M12`의 더 자세한 세부 체크리스트와 완료 기준은 [`docs/m12-checklist.md`](/home/seorii/dev/hancomac/latexd/docs/m12-checklist.md) 에 정리한다.

`2026-03-30` 메모: 이후 web frontend를 `pnpm` workspace + `web/packages/viewer-core` + `web/apps/viewer` 구조로 재패키징하는 infra 작업을 먼저 처리했다. 그 때문에 원래 이어가던 post-`M12` follow-on 중 stronger external editor integration, viewer/editor hardening tail, 그리고 더 넓은 artifact/render ownership 작업은 packaging 이후 다시 잡을 큐로 잠시 밀려 있다.

세부 단계:

* `M12.1` wrapper-heavy corpus baseline
* `M12.2` bibliography/toolchain realism
* `M12.3` package/wrapper interaction + semantic artifact tightening
* `M12.4` failure/recovery + structured artifact coverage
* `M12.5` renderer/session hardening
* `M12.6` sync/editor hardening

실제 완료 기준:

* representative arXiv-style project shape가 wrapper-heavy corpus fixture와 larger paper-family corpus로 올라가 있다.
* bibliography/style/input/tool-version drift, package interaction, semantic replay invalidation이 artifact-level expectation까지 포함해 regression으로 고정돼 있다.
* failure/recovery chain이 `EXPECT`뿐 아니라 `JSON-EXPECT`와 `FAIL-JSON-EXPECT`로도 구조화되어 있다.
* renderer path가 actor-owned revision/session, tile path, prewarm, debug metrics를 가진다.
* source-preview sync와 editor bridge가 page-local source/output range, stable item identity, canonical source hash, richer launch context를 가진다.

현재 구현 상태 메모:

* 이 단계는 더 이상 진행 중 큰 묶음이 아니다. README가 처음 열거한 known-issue fixture 묶음과 small corpus gate를 넘어서, corpus realism, deeper toolchain semantics, renderer/session hardening, richer source-preview sync completion bar까지 채운 상태로 본다.
* 현재 코드베이스는 semantic aux/replay까지 포함한 M11 위에 서 있고, 일부 arXiv-style project model과 local package/class loading은 이미 있지만, full LaTeX-compatible aux/toolchain semantics와 wider corpus compatibility는 여전히 hardening 대상이다.
* README가 열거한 첫 known-issue fixture 묶음인 `\usepackage` in group / `setspace`, `hyperref + hyperxmp` style `\RequirePackage`, `revtex + array` style `\LoadClassWithOptions`, `cleveref + hyperref` style `\AtBeginDocument` / `\DeclareRobustCommand`, `minted` v3 cache style `\IfFileExists{_minted-.../*.pygtex}`, `xelatex` file-name font lookup style `\IfFileExists{Example Font.otf}`는 이미 회귀에 올라가 있고, 그 fixture들은 이제 `fixtures/arxiv-smoke/*`와 `crates/latexd/tests/arxiv_smoke.rs`를 통해 작은 corpus gate로도 함께 검증된다. 같은 local toolchain shim은 `\AtEndDocument`, `\AtEndOfPackage`, `\AtEndOfClass`까지 넓어져 wrapper class/package/document end-hook 설치도 corpus fixture로 고정돼 있다.
* renderer/session hardening도 이제 truly untouched 상태는 아니다. daemon layer에는 같은 document root + renderer mode 요청을 하나의 shared render lane으로 모으는 Phase 1 document render actor/session wrapper가 들어가서, full-page cold miss가 매번 fresh renderer path를 만들지 않게 됐다. 그 위에서 Phase 2도 narrow form을 더 넓혀서 actor가 recent revision page metadata를 attach/detach 하고 attached-page lookup뿐 아니라 full revision page-metadata-set lookup도 먼저 처리할 수 있다. revision retention도 더 이상 build loop의 고정 `rev - N` detach가 아니라 actor-owned LRU + page-budget eviction을 타고, lookup/render/prewarm이 attached revision order를 직접 touch 한다. debug surface에서도 attached revision window 자체를 page-count/page-id 요약뿐 아니라 live attached page count, revision-window limit, page budget, eviction count까지 포함해 볼 수 있다. Phase 3도 narrow form으로 시작돼 `/tiles/*` request가 HTTP-handler crop 대신 actor-owned `render_tiles`를 직접 타게 됐고, actor는 revision/page/content-hash/zoom/rect 기준 small recent tile cache도 들고 있어서 repeated identical tile과 mixed cached+uncached batch에서 missing rect만 다시 렌더한다. 같은 cache는 이제 global LRU budget에 더해 per-page rect budget도 가져서 one page의 rectangle churn이 다른 page tile ownership을 밀어내지 않게 됐고, tile cache eviction도 debug surface에서 counter와 explicit `EvictTileCache` event로 바로 보이게 돼 rect ownership churn을 추적하기 쉬워졌다. cache hit는 여전히 tile-cache count와 explicit `ReuseTiles` event로도 보인다. Phase 4도 narrow form으로 시작돼 viewport prewarm이 current page -> visible pages -> adjacent pages 우선순위로 actor-owned warm request를 보내 session/tile state를 데운 뒤 끝나며 더 이상 eager full-page raster PNG cache fill을 하지 않는다. 같은 Phase 4 narrow path는 same revision/page/zoom bucket의 in-flight warm work도 suppress 해서 concurrent viewport spam이 session actor에 같은 prewarm을 중복으로 밀어 넣지 않게 하고, current page에 대해서는 neighboring zoom-step bucket도 함께 데워서 zoom-in/out 직후 cold miss를 더 줄인다. 최근에는 여기에 actor-owned warm-bucket LRU와 debug exposure가 들어온 데 이어, warm bucket이 이제 per-page zoom-bucket budget도 가져서 one page의 zoom churn이 다른 warmed page state를 밀어내지 않게 됐다. same revision/page/content-hash/zoom bucket prewarm은 attached session 안에서 다시 건너뛸 수 있고 detach/evict 시 stale warmed bucket도 같이 정리된다. Phase 5는 actor spawn/restart/attach/detach/evict/full-render/tile-render/prewarm/fallback counters, render/tile/prewarm/fallback event duration이 실린 recent revision/page keyed event ring, matching structured debug logs, aggregate latency summary가 포함된 debug metrics surface까지 들어왔다. broader rectangle batching/display-list retention, longer-lived warm ownership tuning, richer latency observability는 여전히 남아 있다.
* source-preview sync도 한 칸 더 올라갔다. `/api/syncmap/<rev>/<page_id>`는 이제 per-item box geometry뿐 아니라 page-local `source_start_utf8` / `source_end_utf8` 와 `output_start_utf8` / `output_end_utf8` range도 같이 내보내서, artifact-backed syncmap과 fallback page-metadata path가 둘 다 same page-local source/output window를 직접 드러낸다. sync item 자체도 item-level `output_start_utf8` / `output_end_utf8`와 stable `item_id`를 싣고, artifact/fallback geometry는 page bounds 안으로 normalize 된다. viewer runtime도 fetched syncmap response의 page-local source/output window를 reducer state로 그대로 넘기고, 새 syncmap이 도착했을 때 그 window 밖으로 밀린 stale selection/hover state는 바로 비우며, 같은 `item_id`가 남아 있는 경우에는 selection/hover를 새 geometry/output window로 다시 붙인다. `/api/source-jump/<rev>` 와 `/api/open-source/<rev>` 응답은 이제 둘 다 nearest page/item/page-local source-output window와 page geometry, canonical `source_hash`를 같이 돌려주고, 이 canonical hash는 line-only link에 머무르지 않고 column이 있으면 `&column=`까지 보존한다. viewer는 그 canonical hash를 실제 current page/selected source, source link, URL history, resolved embedding event(`latexd:source-jump-resolved`, `latexd:source-hover-resolved`, `latexd:open-source-resolved`)에 반영하고, explicit `startColumn`이 없는 selection에서도 canonical hash에서 column을 다시 읽어 같은 precision으로 request를 재구성해서 local editor bridge roundtrip에서 selection context를 덜 잃는다. 같은 canonical request input도 이제 `{ sourceHash }` / `{ source_hash }`뿐 아니라 plain `"#src=..."` string으로 직접 넣을 수 있고, `/api/source-jump/<rev>` 와 `/api/open-source/<rev>` 모두 that `source_hash` only input을 받아 nested-path/column selection을 lossy local field 없이 다시 풀 수 있다. `window.latexdOpenSelectedSource(...)`는 그 direct canonical input과 `launch: false`도 같은 imperative entrypoint에서 같이 받을 수 있고, 같은 direct hash/object path는 resolved/failed detail에서도 canonical `source` shape를 다시 세워 주기 때문에 embedding shell이 selected-source path와 direct path를 따로 분기할 필요가 없다. 두 endpoint는 이제 absolute file path, editor cwd, launch-support state, `file_uri`, known-editor `editor_uri`, materialized `editor_program` / `editor_args` / `editor_command_line` preview, 그리고 `editor_preview_kind`(`none` / `uri` / `command` / `command_and_uri`)까지 같은 shape로 돌려줘서 embedding shell이 jump/hover/open-source 어느 path에서든 exact local-editor handoff나 deep-link target을 같은 방식으로 재사용할 수 있다. viewer의 imperative `window.latexdJumpToSource(...)`, `window.latexdSelectSource(...)`, `window.latexdHoverSource(...)` promise도 이제 raw server payload가 아니라 same resolved detail object를 돌려줘서, embedding shell이 event listener 없이도 canonical hash / page-item context / editor preview를 곧바로 재사용할 수 있다. `window.latexdOpenSelectedSource(...)` / `window.latexdPreviewSelectedSource(...)`도 same canonical `source_hash` string이나 object input을 직접 받아 server에 그대로 POST할 수 있다. `/api/open-source/<rev>` editor bridge launch도 같은 richer context를 직접 materialize 하도록 넓어져 `{rev}`, `{source_hash}`, `{page_id}`, `{page_index}`, `{page_width}`, `{page_height}`, `{page_source_start}`, `{page_source_end}`, `{page_output_start}`, `{page_output_end}`, `{item_file}`, `{item_start}`, `{item_end}`, `{item_output_start}`, `{item_output_end}`, `{item_id}`, `{editor_cwd}`, `{absolute_file}`, `{file_uri}`, `{editor_uri}`뿐 아니라 `{editor_program}`, `{editor_command_line}`, `{editor_preview_kind}` placeholder도 command args에 쓸 수 있고, 같은 endpoint는 `launch: false` preview-only path뿐 아니라 no-bridge normal request도 `launched: false` preview payload로 degrade 한다. built-in viewer의 explicit open-source action도 이제 `editorBridgeEnabled=false`에서 intent-only detail로 멈추지 않고 이 resolved payload를 실제로 fetch해서 same event/context surface를 embedding shell에 넘긴다. 그리고 `source-jump` / `source-hover` / `open-source` fetch failure는 각각 `latexd:source-jump-failed`, `latexd:source-hover-failed`, `latexd:open-source-failed`로 request/error context를 실어 보내서 embedding shell이 preview fallback과 actual request failure를 구분할 수 있다.
* corpus realism도 이제 단순 micro-fixture를 넘기 시작했다. split preamble + wrapper option stack + multi-`.bbl` bibliography workflow를 한 fixture 안에 묶어서, revision별 option-only change, bibliography-order change, semantically equal `.bbl` edit, semantic-change `.bbl` edit, partial `.bbl` loss/recovery, nested preamble failure/recovery, local package/class failure/recovery, body include failure/recovery가 concrete `semantic.aux`의 `\bibstyle{...}` / `\bibdata{...}` / `\bibcite{...}` drift와 함께 같이 검증된다. 이 fixture도 이제 `wrapper.cls -> profile.cls -> article.cls`, `shim.sty -> shim.cfg -> shim.def`, `\input{preamble/setup}` -> `\input{preamble/core}` -> `\input{preamble/macros}` / `\input{preamble/theorems}` chain까지 실제로 타는 wrapper-heavy project shape다.
* 같은 corpus realism은 더 큰 mixed-paper fixture 쪽으로도 넓어졌다. dedicated `split-preamble-paper-family-workflow` fixture가 split preamble + wrapper option stack 위에서 short-title TOC, LOF, figure caption/reference, appendix/theorem/`nameref`, natbib author-year citations, bibliography drift, option-only change, semantically equal `.bbl` edit, semantic-change `.bbl` edit, partial `.bbl` loss/recovery, nested preamble/package/class/body failure-recovery를 한 project 안에서 같이 검증한다. 이 fixture는 이제 `wrapper.cls -> profile.cls -> article.cls`, `shim.sty -> shim.cfg -> shim.def`, `\input{preamble/setup}` -> `\input{preamble/core}` -> `\input{preamble/macros}` / `\input{preamble/theorems}` chain까지 실제로 타는 wrapper-heavy project shape다.
* 같은 corpus realism은 manual TOC가 섞인 larger paper-family fixture 쪽으로도 넓어졌다. dedicated `split-preamble-manual-paper-family-workflow` fixture가 split preamble + wrapper option stack 위에서 `\phantomsection` / `\addcontentsline`, short-title TOC, LOF, figure caption/reference, appendix/theorem/`nameref`, natbib author-year citations, bibliography drift, option-only change, semantically equal `.bbl` edit, semantic-change `.bbl` edit, partial `.bbl` loss/recovery, nested preamble/package/class/body failure-recovery를 한 project 안에서 같이 검증한다. 이 fixture도 이제 `wrapper.cls -> profile.cls -> article.cls`, `shim.sty -> shim.cfg -> shim.def`, `\input{preamble/setup}` -> `\input{preamble/core}` -> `\input{preamble/macros}` / `\input{preamble/theorems}` chain까지 실제로 타는 wrapper-heavy project shape다.
* 같은 corpus realism은 biblatex mixed-paper fixture 쪽으로도 넓어졌다. dedicated `split-preamble-biblatex-paper-family-workflow` fixture가 split preamble + wrapper option stack 위에서 short-title TOC, LOF, figure caption/reference, appendix/theorem/`nameref`, `\textcite` / `\parencite` / `\printbibliography`, bibliography-order drift, semantically equal `.bbl` edit, semantic-change `.bbl` edit, partial `.bbl` loss/recovery, nested preamble/package/class/body failure-recovery를 한 project 안에서 같이 검증한다. 이 fixture는 이제 `wrapper.cls -> profile.cls -> article.cls`, `shim.sty -> shim.cfg -> shim.def`, `\input{preamble/setup}` -> `\input{preamble/core}` -> `\input{preamble/macros}` / `\input{preamble/theorems}` chain까지 실제로 타는 wrapper-heavy project shape다.
* 같은 corpus realism은 manual TOC가 섞인 biblatex larger paper-family fixture 쪽으로도 넓어졌다. dedicated `split-preamble-manual-biblatex-paper-family-workflow` fixture가 split preamble + wrapper option stack 위에서 `\phantomsection` / `\addcontentsline`, short-title TOC, LOF, figure caption/reference, appendix/theorem/`nameref`, `\textcite` / `\parencite` / `\printbibliography`, bibliography-order drift, semantically equal `.bbl` edit, semantic-change `.bbl` edit, partial `.bbl` loss/recovery, nested preamble/package/class/body failure-recovery를 한 project 안에서 같이 검증한다. 이 fixture도 이제 `wrapper.cls -> profile.cls -> article.cls`, `shim.sty -> shim.cfg -> shim.def`, `\input{preamble/setup}` -> `\input{preamble/core}` -> `\input{preamble/macros}` / `\input{preamble/theorems}` chain까지 실제로 타는 wrapper-heavy project shape다.
* same larger mixed-paper corpus도 한 칸 더 올라갔다. dedicated `split-preamble-mixed-paper-family-workflow` fixture가 split preamble + wrapper option stack 위에서 theorem + cleveref + appendix + LOF/figure caption/list + bibliography drift를 한 project 안에 섞고, option-only change와 nested preamble/package/class/body failure-recovery까지 같이 고정한다. 이 fixture 역시 `wrapper.cls -> profile.cls -> article.cls`, `shim.sty -> shim.cfg -> shim.def`, `\input{preamble/setup}` -> `\input{preamble/core}` -> `\input{preamble/macros}` / `\input{preamble/theorems}` chain을 실제로 타는 wrapper-heavy project shape다.
* same larger mixed-paper corpus도 reference/manual-heading 쪽으로 더 넓어졌다. `split-preamble-reference-mixed-paper-family-workflow` fixture는 split preamble + wrapper option stack 위에서 equation/float/label/range/title refs와 cleveref/capitalized variants를 한 project 안에 섞고 option-only change와 nested preamble/package/class/body failure-recovery까지 같이 고정한다. `split-preamble-manual-bibliography-caption-workflow` fixture는 같은 wrapper-heavy project shape 위에서 manual TOC, bibliography heading, figure caption/list, theorem/appendix/reference semantics를 한 project 안에 섞고 title drift, bibliography drift, partial `.bbl` loss/recovery, nested preamble/package/class/body failure-recovery까지 같이 검증한다. `split-preamble-appendix-reference-paper-family-workflow` fixture는 appendix/appendices/theorem/float/equation/range/title refs를 같은 larger family 안에 함께 묶고, `split-preamble-manual-heading-reference-paper-family-workflow` fixture는 manual TOC + bibliography heading + float/caption/list + theorem/shared-counter + title/page/range refs를 partial `.bbl` loss/recovery와 nested preamble/package/class/body failure-recovery까지 포함한 same wrapper-heavy chain으로 고정한다.
* same larger mixed-paper corpus도 theorem/package-wrapper archetype 쪽으로 더 넓어졌다. `split-preamble-heading-theorem-paper-family-workflow` fixture는 split preamble + wrapper option stack 위에서 manual TOC, bibliography heading, theorem/shared-counter drift, appendix, figure/captionof/list semantics를 한 project 안에 섞고 semantically equal `.bbl`, semantic-change `.bbl`, partial `.bbl` loss/recovery, nested preamble/package/class/body failure-recovery까지 같이 고정한다. `split-preamble-package-wrapper-paper-family-workflow` fixture는 같은 wrapper-heavy project shape 위에서 `hyperref + cleveref + varioref + theorem/appendix/float/reference` interaction, short-title/manual TOC, equation/float/label/range/title refs, option-only drift, nested preamble/package/class/body failure-recovery를 한 project 안에 같이 묶는다.
* same larger mixed-paper corpus도 bibliography/appendix-heavy archetype 쪽으로 더 넓어졌다. `split-preamble-bibliography-heavy-paper-family-workflow` fixture는 split preamble + wrapper option stack 위에서 multi-`.bbl` / `\addbibresource` bibliography workflow, manual/frontmatter heading drift, bibliography order/style drift, semantically equal `.bbl`, semantic-change `.bbl`, partial `.bbl` loss/recovery, nested preamble/package/class/body failure-recovery를 한 project 안에 같이 고정한다. `split-preamble-appendix-heavy-paper-family-workflow` fixture는 같은 wrapper-heavy project shape 위에서 appendix/appendices/theorem/caption/list/reference mix, option-only drift, nested preamble/package/class/body failure-recovery를 한 project 안에 같이 묶는다. `split-preamble-citation-caption-reference-heavy-paper-family-workflow` fixture는 같은 wrapper-heavy chain 위에서 natbib author-year citations, figure/captionof/list semantics, title/page/range/theorem refs, bibliography drift, partial `.bbl` loss/recovery, nested preamble/package/class/body failure-recovery를 한 project 안에 같이 고정한다. `split-preamble-biblatex-heading-reference-heavy-paper-family-workflow` fixture는 같은 wrapper-heavy chain 위에서 biblatex citation stack, bibliography heading/manual-frontmatter drift, caption/list semantics, title/page/range refs, bibliography drift, partial `.bbl` loss/recovery, nested preamble/package/class/body failure-recovery를 한 project 안에 같이 고정한다. `split-preamble-manual-heading-caption-reference-heavy-paper-family-workflow` fixture는 같은 wrapper-heavy chain 위에서 manual TOC + bibliography heading + caption/list + title/page/range/reference stack을, `split-preamble-varioref-reference-heavy-paper-family-workflow` fixture는 `varioref` + theorem/equation/float/title/page/range/reference stack을 같은 partial-loss and failure/recovery chain 안에 함께 묶는다.
* same larger mixed-paper corpus는 toolchain/package/failure archetype으로도 더 넓어졌다. `split-preamble-bibliography-toolversion-paper-family-workflow` fixture는 bibliography tool-version/style/input drift와 semantic/failure-recovery chain을 larger wrapper-heavy family 안에 직접 고정하고, `split-preamble-package-interaction-paper-family-workflow` fixture는 `hyperref + cleveref + varioref + theorem/appendix/float/reference` interaction을 larger paper family 안에 고정하며, `split-preamble-failure-semantic-chain-paper-family-workflow` fixture는 semantic-change rebuild와 package/class/body failure-recovery를 one longer chain으로 묶는다.
* 같은 larger paper-family recovery chain은 structured build metadata까지 더 깊게 고정된다. `split-preamble-paper-family-workflow`, `split-preamble-biblatex-paper-family-workflow`, `split-preamble-manual-paper-family-workflow`, `split-preamble-manual-biblatex-paper-family-workflow`의 recovery rev(`REV10/12/14/16`)는 이제 `semantic_aux_backdated`뿐 아니라 `rebuilt_page_count`, `reused_page_count`, `semantic_pass_count`도 `build-meta.json` 기준으로 직접 검증한다.
* 같은 corpus realism은 biblatex-style citation surface 쪽으로도 넓어졌다. dedicated `split-preamble-biblatex-citation-workflow` fixture가 split preamble + wrapper option stack 위에서 `\textcite`, `\parencite`, `\printbibliography`, bibliography drift, option-only change, nested preamble/package/class/body failure-recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 biblatex multi-cite surface 쪽으로도 넓어졌다. dedicated `split-preamble-biblatex-multicite-workflow` fixture가 split preamble + wrapper option stack 위에서 `\textcites`, `\parencites`, `\printbibliography`, option-only change, nested preamble/package/class/body failure-recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 bibliography metadata/field surface 쪽으로도 넓어졌다. dedicated `split-preamble-bibliography-metadata-workflow` fixture가 split preamble + wrapper option stack 위에서 `\citeauthor`, `\citeyear`, `\citetitle`, `\citefield{doi,eprint}`, bibliography drift, partial `.bbl` loss, nested preamble/package/class/body failure-recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 bibliography date/urldate surface 쪽으로도 넓어졌다. dedicated `split-preamble-bibliography-date-workflow` fixture가 split preamble + wrapper option stack 위에서 `\citedate`, `\Citedate`, `\citeurldate`, `\Citeurldate`, bibliography drift, partial `.bbl` loss, nested preamble/package/class/body failure-recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 direct bibliography identifier surface 쪽으로도 넓어졌다. dedicated `split-preamble-bibliography-identifier-workflow` fixture가 split preamble + wrapper option stack 위에서 `\citedoi`, `\citeeprint`, `\citeisbn`, `\citeissn`, `\citeurl`, bibliography drift, partial `.bbl` loss, nested preamble/package/class/body failure-recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 bibliography year-suffix surface 쪽으로도 넓어졌다. dedicated `split-preamble-bibliography-year-suffix-workflow` fixture가 split preamble + wrapper option stack 위에서 `\citeyear`, `\citeyearpar*`, `\natexlab` / `\NAT@exlab`-style suffix drift, partial `.bbl` loss, nested preamble/package/class/body failure-recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 capitalized bibliography author/year/title surface 쪽으로도 넓어졌다. dedicated `split-preamble-bibliography-capitalized-workflow` fixture가 split preamble + wrapper option stack 위에서 `\Citeauthor`, `\Citeyear`, `\Citeauthor*`, `\Citeyear*`, `\Citetitle`, bibliography drift, partial `.bbl` loss, nested preamble/package/class/body failure-recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 generic bibliography field surface 쪽으로도 넓어졌다. dedicated `split-preamble-bibliography-generic-field-workflow` fixture가 split preamble + wrapper option stack 위에서 `\citefield{author}`, `\citefield{year}`, `\citefield{title}`, `\citefield{label}`, `\citefield{journal}`, `\citefield{pages}`, bibliography drift, partial `.bbl` loss, nested preamble/package/class/body failure-recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 bibliography fullauthor/text surface 쪽으로도 넓어졌다. dedicated `split-preamble-bibliography-fullauthor-workflow` fixture가 split preamble + wrapper option stack 위에서 `\citefield{fullauthor}`, `\citefield{text}`, `\citefield{labelname}`, bibliography drift, partial `.bbl` loss, nested preamble/package/class/body failure-recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 bibliography inclusion-only surface 쪽으로도 넓어졌다. dedicated `split-preamble-nocite-workflow` fixture가 split preamble + wrapper option stack 위에서 `\nocite{alpha}`와 later `\nocite{*}` bibliography expansion, bibliography/style drift, partial `.bbl` loss, nested preamble/package/class/body failure-recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 natbib textual/parenthetical surface 쪽으로도 넓어졌다. dedicated `split-preamble-natbib-textual-workflow` fixture가 split preamble + wrapper option stack 위에서 `\citet`, `\citep`, `\citealt`, `\citealp`, `\onlinecite`, `\citetext`, `\citeyearpar`, bibliography drift, partial `.bbl` loss, nested preamble/package/class/body failure-recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 natbib alias/full-author/numeric surface 쪽으로도 넓어졌다. dedicated `split-preamble-natbib-alias-workflow` fixture가 split preamble + wrapper option stack 위에서 `\defcitealias`, `\citetalias`, `\citepalias`, `\Citetalias`, `\citefullauthor`, `\Citefullauthor*`, `\Citeyearpar`, `\citenum`, bibliography drift, partial `.bbl` loss, nested preamble/package/class/body failure-recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 natbib starred textual surface 쪽으로도 넓어졌다. dedicated `split-preamble-natbib-starred-workflow` fixture가 split preamble + wrapper option stack 위에서 `\citet*`, `\citep*`, `\citealt*`, `\citealp*`, `\Textcite*`, bibliography drift, partial `.bbl` loss, nested preamble/package/class/body failure-recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 natbib capitalized textual surface 쪽으로도 넓어졌다. dedicated `split-preamble-natbib-capitalized-workflow` fixture가 split preamble + wrapper option stack 위에서 `\Citet`, `\Citep`, `\Citealt`, `\Citealp`, `\Textcite`, bibliography drift, partial `.bbl` loss, nested preamble/package/class/body failure-recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 biblatex auto/footcite surface 쪽으로도 넓어졌다. dedicated `split-preamble-biblatex-autofoot-workflow` fixture가 split preamble + wrapper option stack 위에서 `\autocite`, `\autocites`, `\footcite`, `\footcites`, bibliography drift, partial `.bbl` loss, nested preamble/package/class/body failure-recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 biblatex plain multi-cite surface 쪽으로도 넓어졌다. dedicated `split-preamble-biblatex-cites-workflow` fixture가 split preamble + wrapper option stack 위에서 `\cites`, option-only change, nested preamble/package/class/body failure-recovery를 한 project 안에서 같이 검증한다.
* same-page semantic rewrite가 bibliography input checkpoint보다 더 이르게 변하는 경우의 replay hardening도 보강됐다. 현재 shipout replay selector는 override-only semantic diff가 있으면 candidate input-boundary checkpoint의 `output_start_utf8`가 earliest changed rewrite output보다 늦지 않도록 제한해서, partial bibliography loss 뒤 recovery revision에서 stale cite fallback/tail이 output에 남는 문제를 dedicated smoke와 larger biblatex paper-family corpus로 같이 막는다.
* 같은 invalidation hardening은 semantic-changing bibliography input에도 넓어졌다. changed `.bbl`이 current bibliography set에 포함되고 semantic aux가 실제로 drift하는 revision은 final semantic build를 reused preamble checkpoint가 아니라 base snapshot에서 다시 돌려서, preamble bibliography text가 있는 workflow에서도 stale bibliography prefix가 남지 않게 한다. 반대로 semantically-equal later `.bbl` edits는 그대로 page/checkpoint reuse를 유지한다.
* 같은 corpus realism은 biblatex `\addbibresource`/`\printbibliography` surface 쪽으로도 넓어졌다. dedicated `split-preamble-biblatex-addbibresource-workflow` fixture가 split preamble + wrapper option stack 위에서 `\addbibresource{refs-a.bib}`, `\addbibresource{refs-b.bib}`, `\textcite`, `\parencite`, `\printbibliography`, bibliography-order drift, partial `.bbl` loss, nested preamble/package/class/body failure-recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 biblatex entry/smartcite surface 쪽으로도 넓어졌다. dedicated `split-preamble-biblatex-entry-workflow` fixture가 split preamble + wrapper option stack 위에서 `\smartcite`, `\smartcites`, `\fullcite`, `\bibentry`, bibliography drift, partial `.bbl` loss, nested preamble/package/class/body failure-recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 biblatex `\footfullcite` surface 쪽으로도 넓어졌다. dedicated `split-preamble-biblatex-footfullcite-workflow` fixture가 split preamble + wrapper option stack 위에서 `\smartcite`, `\footfullcite`, bibliography drift, partial `.bbl` loss, nested preamble/package/class/body failure-recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 biblatex supercite surface 쪽으로도 넓어졌다. dedicated `split-preamble-biblatex-supercite-workflow` fixture가 split preamble + wrapper option stack 위에서 `\supercite`, `\supercites`, bibliography drift, partial `.bbl` loss, nested preamble/package/class/body failure-recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 bibliography 말고도 split preamble + wrapper option stack + TOC/LOF + float/list/reference semantics 쪽으로 넓어졌다. dedicated `split-preamble-float-reference-workflow` fixture가 caption short/long title drift, option-only change, nested preamble failure/recovery, local package/class failure/recovery, body include failure/recovery를 한 project 안에서 같이 검증한다. 이 fixture도 이제 `wrapper.cls -> profile.cls -> article.cls`, `shim.sty -> shim.cfg -> shim.def`, `\input{preamble/setup}` -> `\input{preamble/core}` -> `\input{preamble/macros}` chain까지 실제로 타는 wrapper-heavy project shape다.
* 같은 corpus realism은 `\captionof`-driven float/list/reference semantics 쪽으로도 넓어졌다. dedicated `split-preamble-captionof-workflow` fixture가 split preamble + wrapper option stack 위에서 `\captionof{figure}[Short]{Long}` drift, `\listoffigures`, `\autoref` / `\cref` / `\namecref` / `\nameref`, option-only change, nested preamble failure/recovery, local package/class failure/recovery, body include failure/recovery를 한 project 안에서 같이 검증한다. 이 fixture도 이제 `wrapper.cls -> profile.cls -> article.cls`, `shim.sty -> shim.cfg -> shim.def`, `\input{preamble/setup}` -> `\input{preamble/core}` -> `\input{preamble/macros}` chain까지 실제로 타는 wrapper-heavy project shape다.
* 같은 corpus realism은 appendix/theorem/reference semantics 쪽으로도 넓어졌다. dedicated `split-preamble-appendix-theorem-workflow` fixture가 appendix lettering, TOC short-title drift, theorem numbering/title drift, option-only change, nested theorem-definition failure/recovery, local package/class failure/recovery, body include failure/recovery를 split preamble + wrapper option stack 위에서 한 project 안에 같이 묶는다. 이 fixture도 이제 `wrapper.cls -> profile.cls -> article.cls`, `shim.sty -> shim.cfg -> shim.def`, `\input{preamble/setup}` -> `\input{preamble/core}` -> `\input{preamble/macros}` / `\input{preamble/theorems}` chain까지 실제로 타는 wrapper-heavy project shape다.
* 같은 corpus realism은 equation/reference semantics 쪽으로도 넓어졌다. dedicated `split-preamble-equation-reference-workflow` fixture가 split preamble + wrapper option stack 위에서 equation label drift, `\eqref` / `\autoref` / `\cref` / `\namecref` / `\vref` / `\crefrange`, option-only change, nested preamble failure/recovery, local package/class failure/recovery, body include failure/recovery를 한 project 안에서 같이 검증한다. 이 fixture도 이제 `wrapper.cls -> profile.cls -> article.cls`, `shim.sty -> shim.cfg -> shim.def`, `\input{preamble/setup}` -> `\input{preamble/core}` -> `\input{preamble/macros}` chain까지 실제로 타는 wrapper-heavy project shape다.
* 같은 corpus realism은 sub-reference semantics 쪽으로도 넓어졌다. dedicated `split-preamble-subref-subeqref-workflow` fixture가 split preamble + wrapper option stack 위에서 `\subref`, `\subeqref`, figure/equation numbering drift, option-only change, nested preamble failure/recovery, local package/class failure/recovery, body include failure/recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 split-document/includeonly semantics 쪽으로도 넓어졌다. dedicated `split-preamble-includeonly-workflow` fixture가 split preamble + wrapper option stack 위에서 `\includeonly` set drift, TOC drift, option-only change, nested preamble failure/recovery, local package/class failure/recovery, included body failure/recovery를 한 project 안에서 같이 검증한다. 이 fixture도 이제 `wrapper.cls -> profile.cls -> article.cls`, `shim.sty -> shim.cfg -> shim.def`, `\input{preamble/setup}` -> `\input{preamble/core}` -> `\input{preamble/macros}` chain까지 실제로 타는 wrapper-heavy project shape다.
* 같은 corpus realism은 manual TOC entry semantics 쪽으로도 넓어졌다. dedicated `split-preamble-manual-toc-workflow` fixture가 split preamble + wrapper option stack 위에서 `\phantomsection` / `\addcontentsline` title drift, option-only change, nested preamble failure/recovery, local package/class failure/recovery, body include failure/recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 heading-only bibliography semantics 쪽으로도 넓어졌다. dedicated `split-preamble-printbibheading-workflow` fixture가 split preamble + wrapper option stack 위에서 `\printbibheading[heading=bibintoc,title=...]` title drift, option-only change, nested preamble failure/recovery, local package/class failure/recovery, bibliography body include failure/recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 manual TOC + heading-only bibliography + multi-`.bbl` mixed stack 쪽으로도 넓어졌다. dedicated `split-preamble-bibliography-heading-manual-workflow` fixture가 split preamble + wrapper option stack 위에서 `\phantomsection` / `\addcontentsline`, `\printbibheading[heading=bibintoc,title=...]`, multi-`.bbl` bibliography/style drift, partial `.bbl` loss, nested preamble/package/class/body failure-recovery를 한 project 안에서 같이 검증한다. 이 fixture도 이제 `wrapper.cls -> profile.cls -> article.cls`, `shim.sty -> shim.cfg -> shim.def`, `\input{preamble/setup}` -> `\input{preamble/core}` -> `\input{preamble/macros}` / `\input{preamble/theorems}` chain까지 실제로 타는 wrapper-heavy project shape다.
* 같은 corpus realism은 page-range / label-only reference semantics 쪽으로도 넓어졌다. dedicated `split-preamble-range-label-reference-workflow` fixture가 split preamble + wrapper option stack 위에서 `\pagerefrange` / `\vpagerefrange` / `\vrefrange` / `\cpagerefrange` / `\labelcref` / `\labelcpageref`, label drift, option-only change, nested preamble failure/recovery, local package/class failure/recovery, body include failure/recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 capitalized range/page reference semantics 쪽으로도 넓어졌다. dedicated `split-preamble-capitalized-range-reference-workflow` fixture가 split preamble + wrapper option stack 위에서 `\vpageref`, `\Cpagerefrange`, `\Vrefrange`, label-kind drift, option-only change, nested preamble failure/recovery, local package/class failure/recovery, body include failure/recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 capitalized single-name cleveref semantics 쪽으로도 넓어졌다. dedicated `split-preamble-capitalized-cref-workflow` fixture가 split preamble + wrapper option stack 위에서 `\Cref`, `\nameCref`, equation numbering drift, option-only change, nested preamble failure/recovery, local package/class failure/recovery, body include failure/recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 title/page-oriented reference semantics 쪽으로도 넓어졌다. dedicated `split-preamble-title-page-reference-workflow` fixture가 split preamble + wrapper option stack 위에서 `\fullref` / `\Fullref` / `\titleref` / `\Titleref` / `\namecref` / `\nameCref` / `\namecrefs` / `\lcnamecref` / `\cpageref` / `\Cpageref` / `\autopageref` / `\vref` / `\Vref`, section/subsection/theorem title drift, option-only change, nested preamble failure/recovery, local package/class failure/recovery, body include failure/recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 short-title / `\nameref` semantics 쪽으로도 넓어졌다. dedicated `split-preamble-short-title-workflow` fixture가 split preamble + wrapper option stack 위에서 `\section[Short]{Long}` TOC drift, body long-title preservation, `\nameref` title drift, option-only change, nested preamble failure/recovery, local package/class failure/recovery, body include failure/recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 book-style appendix/autoref semantics 쪽으로도 넓어졌다. dedicated `split-preamble-book-appendix-workflow` fixture가 split preamble + wrapper option stack 위에서 `\appendix` chapter lettering, appendix TOC drift, chapter/section appendix `\autoref`, option-only change, nested preamble failure/recovery, local package/class failure/recovery, body include failure/recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 theoremstyle/custom-theorem semantics 쪽으로도 넓어졌다. dedicated `split-preamble-theoremstyle-workflow` fixture가 split preamble + wrapper option stack 위에서 `\theoremstyle` / `\newtheoremstyle` / `\swapnumbers`, custom theorem/shared-counter drift, theorem title drift, option-only change, nested preamble failure/recovery, local package/class failure/recovery, body include failure/recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 `\appendices` synonym semantics 쪽으로도 넓어졌다. dedicated `split-preamble-appendices-workflow` fixture가 split preamble + wrapper option stack 위에서 appendix lettering, appendix TOC drift, subsection appendix `\autoref`, `\nameref`, option-only change, nested preamble failure/recovery, local package/class failure/recovery, body include failure/recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 theorem multi-ref semantics 쪽으로도 넓어졌다. dedicated `split-preamble-theorem-multiref-workflow` fixture가 split preamble + wrapper option stack 위에서 repeated theorem numbering, `\autoref`, plural `\cref`, plural `\namecrefs`, `\nameref`, option-only change, nested preamble failure/recovery, local package/class failure/recovery, body include failure/recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 capitalized/plural cleveref semantics 쪽으로도 넓어졌다. dedicated `split-preamble-capitalized-cleveref-workflow` fixture가 split preamble + wrapper option stack 위에서 `\Crefrange`, `\nameCrefs`, `\lcnamecrefs`, subsection/paragraph/theorem-kind numbering drift, option-only change, nested preamble failure/recovery, local package/class failure/recovery, body include failure/recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 theorem-kind reference semantics 쪽으로도 넓어졌다. dedicated `split-preamble-thmref-workflow` fixture가 split preamble + wrapper option stack 위에서 `\thmref`, `\Thmref`, `\namecref`, `\vref`, theorem/shared-counter declaration drift, option-only change, nested preamble failure/recovery, local package/class failure/recovery, body include failure/recovery를 한 project 안에서 같이 검증한다.
* 같은 corpus realism은 `\newtheorem` declaration drift semantics 쪽으로도 넓어졌다. dedicated `split-preamble-theorem-declaration-workflow` fixture가 split preamble + wrapper option stack 위에서 theorem/shared-counter declaration change, `\autoref`, option-only change, nested preamble failure/recovery, local package/class failure/recovery, body include failure/recovery를 한 project 안에서 같이 검증한다.
* 같은 narrow toolchain shim은 이제 `\InputIfFileExists`, `\makeatletter`-scoped `\@input`, `\newread`, plain `\openin`, plain `\closein`, `\ifeof`, plain `\read`, plain `\readline`, plain `\endlinechar`, `\newwrite`, plain `\openout`, plain `\closeout`, plain `\write`, `\immediate`, `\protected@write`, `\newtoks`, `\toksdef`, plain `\toks`, `\newdimen`, `\dimendef`, plain `\dimen`, public `\newlength`, `\setlength`, `\addtolength`, `\endinput`, `\ProvidesFile`까지 넓어졌고, package/class/generic preamble info-warning-error helper, `\message` / `\typeout` / `\wlog`, whitespace/name helper(`\ignorespaces`, `\jobname`, `\makeatletter`-scoped `\@currname/\@currext/\@currpath`, `\filename@parse` + `\filename@area/\@base/\@ext`), explicit grouping helper(`\begingroup`, `\endgroup`, `\bgroup`, `\egroup`, `\aftergroup`), assignment-complete helper(`\afterassignment`), boolean-definition helper(`\newif`, builtin `\iftrue/\iffalse`, `\makeatletter`-scoped `\@fileswtrue/\@fileswfalse/\if@filesw`, plain `\nofiles`), numeric/character/token-kind conditional helper(`\unless`, `\if`, `\ifodd`, `\ifcat`, `\ifcase...\or...\else...\fi`), loaded/version/branch/name helper(`\IfPackageLoadedTF`, `\IfClassLoadedTF`, `\IfPackageAtLeastTF`, `\IfClassAtLeastTF`, `\ifcsname...\endcsname`, `\makeatletter`-scoped `\@ifundefined`, `\@ifdefinable`, `\@onlypreamble`, `\@onelevel@sanitize`, `\@bsphack`, `\@esphack`, `\in@`, `\ifin@`, `\@ifpackageloaded`, `\@ifclassloaded`, `\@ifpackagewith`, `\@ifclasswith`, `\@ifpackagelater`, `\@ifclasslater`, `\@ifempty`, `\@ifnotempty`, `\@ifmtarg`, `\@ifnotmtarg`, `\@firstofone`, `\@iden`, `\@firstoftwo`, `\@secondoftwo`, `\@gobble`, `\@gobbletwo`, `\@gobblethree`, `\@gobblefour`, `\g@addto@macro`, `\@namedef`, `\@namexdef`, `\@nameuse`, `\@xp`, `\@xa`, `\@ifnextchar`, `\@ifstar`, `\kernel@ifnextchar`, `\kernel@ifstar`, `\@testopt`, `\@dblarg`, `\@car`, `\@cdr`, `\@tfor`, `\@cons`, `\@removeelement`, `\@thirdofthree`, `\@expandtwoargs`, `\@arabic`, `\@roman`, `\@Roman`, `\@alph`, `\@Alph`, builtin `\space`, `\@backslashchar`, `\@percentchar`, `\@hashchar`, `\zap@space`, `\@tempswatrue`, `\@tempswafalse`, `\if@tempswa`, `\@for`, `\@whilenum`, `\@whilesw`, builtin `\@empty/\@nil`)도 좁게 받는다. `\ProvidesPackage` / `\ProvidesClass` / `\ProvidesFile`는 tracked provided-date map과 `\ver@...` low-level version macro뿐 아니라 narrow `\filedate` / `\fileversion` / `\fileinfo` metadata도 함께 만들고, `\IfFileExists` / `\InputIfFileExists`는 found path를 `\makeatletter`-scoped `\@filef@und`에 좁게 남기며, loaded package/class option set도 snapshot-aware state로 유지한다. local wrapper package/class가 optional `*.cfg`/asset 파일을 input하면서 success/fallback branch를 고르거나 `\@input`으로 existing file만 조용히 끌어오고 missing file은 무시하거나, `\newread`로 distinct read-stream alias를 만들고 plain `\openin` / `\closein`으로 file-open payload를 non-visible하게 소비하거나 `\ifeof`로 unopened/missing/open/closed stream state를 snapshot-aware conditional로 읽거나 plain `\read ... to ...`와 `\readline ... to ...`로 다음 config line을 control sequence에 materialize 하되 plain `\endlinechar`가 `0..=255`일 때는 그 visible suffix를 tokenization 직전에 덧붙이고 EOF에서는 `\relax` meaning으로 떨어뜨리거나, `\newwrite`로 distinct stream alias를 만들고 plain `\openout` / `\closeout` / `\write`, `\immediate\write`, `\protected@write`로 aux-style write payload를 non-visible하게 소비하거나, `\newtoks`로 distinct token-register alias를 만들고 `\toksdef`로 explicit token-register alias를 바인딩하고 plain `\toks` assignment로 grouped token list를 snapshot-aware register state에 저장하고 `\the`가 count register뿐 아니라 token register와 dimen/length register도 narrow-materialize 하거나, `\newdimen` / `\dimendef` / plain `\dimen`과 public `\newlength` / `\setlength` / `\addtolength`로 snapshot-aware dimen state를 `pt` text까지 포함해 좁게 다루고 builtin scratch dimen aliases(`\dimen@`, `\@tempdima`, `\@tempdimb`)와 `\p@`를 common package internals에 제공하거나, `\makeatletter`-scoped `\@fileswtrue/\@fileswfalse/\if@filesw`와 plain `\nofiles`로 aux-write enable state를 snapshot-aware boolean으로 읽고 토글하거나, trailing tokens 앞에서 `\endinput`으로 조기 종료하고, `\PackageInfoNoLine` / `\ClassInfoNoLine` / `\PackageWarningNoLine` / `\ClassWarningNoLine`, `\GenericInfo` / `\GenericWarning`, `\makeatletter`-scoped `\@latex@info` / `\@latex@warning` / `\@latex@warning@no@line`, `\message` / `\typeout` / `\wlog`, `\ignorespaces`, `\jobname`, `\@currname/\@currext/\@currpath`, `\filename@parse`, loaded/version/branch/name helper를 써도 VM regression과 `arxiv-smoke` corpus fixture에서 그대로 통과하고, `\PackageError` / `\ClassError` / `\GenericError` / plain TeX `\errmessage` / `\makeatletter`-scoped `\@latex@error` / `\@latexerr` 같은 fatal helper는 expected failure/recovery fixture로 고정된다.
* 같은 corpus gate는 low-level sanitize/no-op + empty-argument helper cluster도 이제 함께 핀다. dedicated wrapper class/package fixture가 `\@onlypreamble`, `\@onelevel@sanitize`, `\@bsphack`, `\@esphack`, `\zap@space`, `\@ifempty`, `\@ifnotempty`, `\@ifmtarg`, `\@ifnotmtarg`를 later option-only revision까지 한 project 안에서 같이 검증한다.
* 같은 command declaration family에는 guard semantics도 올라가 있다. 현재 구현은 `\newcommand`와 `\DeclareRobustCommand`가 이미 정의된 target에서 silent redefine 대신 explicit diagnostic을 내고, `\renewcommand`는 missing target에서 silent define 대신 explicit diagnostic을 내며, `\providecommand`는 기존 target을 그대로 두는 좁은 LaTeX-style path를 유지한다. dedicated wrapper class/package corpus fixture가 `renewcommand missing`과 `newcommand duplicate` failure/recovery chain을 later revision까지 고정한다.
* 같은 low-level dimension/glue branch guard에는 `\ifdim`도 포함된다. 현재 구현은 dimen register, skip/glue register, `\p@` 같은 dimen-like helper를 common dimension expression reader 위에서 좁게 비교하는 semantics이고, wrapper class/package option stack 위의 dedicated corpus fixture로 later option-only revision까지 고정돼 있다.
* 같은 narrow toolchain shim은 지금 narrow glue/skip register path도 포함한다. `\newskip`, `\skipdef`, plain `\skip`은 snapshot-aware skip register alias와 assignment path를 만들고, glue literal의 narrow `plus` / `minus` tail은 base `pt` value를 유지한 채 소비되며, builtin scratch skip aliases(`\skip@`, `\@tempskipa`, `\@tempskipb`)와 `\the`의 conservative `pt` materialization까지 wrapper class/package corpus fixture로 later option-only revision까지 고정돼 있다.
* 같은 narrow toolchain shim은 지금 dimen/glue arithmetic path도 포함한다. `\advance`는 더 이상 count register에만 좁게 머무르지 않고 dimen register와 skip/glue register도 같은 assignment path 위에서 누적 갱신하며, `\multiply` / `\divide`도 count path와 같은 narrow arithmetic helper를 dimen/skip register에 재사용한다. `\newdimen` / `\newskip` aliases, builtin scratch dimen/skip helpers, `\the`의 `pt` readback까지 wrapper class/package corpus fixture로 later option-only revision까지 고정돼 있다.
* 같은 low-level character-definition path에는 `\chardef`도 포함된다. 현재 구현은 visible character token과 number-expression alias를 함께 정의하는 narrow semantics이고, TeX-style backtick char constant도 같은 integer reader에서 좁게 처리하며, dedicated wrapper class/package corpus fixture가 direct output과 `\number` / `\edef` capture path를 later option-only revision까지 고정한다.
* 같은 low-level stringification path에는 `\escapechar`도 포함된다. 현재 구현은 plain `\escapechar`를 count-alias처럼 읽고 쓰며, `\string`이 runtime과 `\edef` capture 양쪽에서 그 prefix를 좁게 따르고, `\detokenize` / `\meaning`도 같은 prefix를 재사용하는 semantics이다. dedicated wrapper class/package corpus fixture가 revision별 prefix change와 later option-only revision까지 고정한다.
* 같은 low-level line-ending path에는 `\endlinechar`도 포함된다. 현재 구현은 plain `\endlinechar`를 count-alias처럼 읽고 쓰며, `\read ... to ...`와 `\readline ... to ...`가 values in `0..=255`일 때 그 visible line-ending character를 tokenization 직전에 덧붙이고 negative 값이면 기존 no-suffix path를 유지하는 semantics이다. dedicated wrapper class/package corpus fixture가 revision별 line-ending suffix change와 later option-only revision까지 고정한다.
* 같은 low-level definition-scope path에는 `\globaldefs`도 포함된다. 현재 구현은 plain `\globaldefs`를 count-alias처럼 읽고 쓰며, positive 값이면 grouped `\def` / `\let` / `\newif` / `\newcommand` 계열을 global처럼 올리고 negative 값이면 explicit `\global`까지 local로 눌러 버리는 좁은 override semantics이다. dedicated wrapper class/package corpus fixture가 option-driven grouped definition persistence를 later option-only revision까지 고정한다.
* 같은 preamble-definition shim에는 `\edef`, `\xdef`, `\gdef`, `\protected@edef`, `\protected@xdef`, `\protect`, `\makeatletter`-scoped `\@typeset@protect` / `\@unexpandable@protect`, `\uppercase`, `\lowercase`, `\strip@prefix`도 포함된다. 현재 구현은 `\edef` body를 좁게 fully-expand 해서 definition time payload로 굳히고, `\xdef`는 그 expanded payload를 grouped global definition으로 남기며, `\gdef`는 grouped global persistence만 담당한다. `\protected@edef`와 `\protected@xdef`는 지금 narrow scaffold에서 같은 expansion/global-definition path를 makeatletter alias로 재사용한다. 같은 balanced-group expansion loop는 `\expanded{...}`에서도 재사용되고, `\protect`, `\noexpand`, makeatletter alias `\@nx`, `\@typeset@protect`, `\@unexpandable@protect`, balanced-group `\unexpanded{...}`도 좁게 받기 때문에 `\edef`/`\xdef` 안이나 definition-time outside path에서 literal control sequence나 grouped token list를 runtime까지 보존하는 common package-internal pattern도 wrapper class/package option stack fixture로 later option-only revision까지 고정돼 있다. plain execution에서 `\protect`와 `\@typeset@protect`는 non-visible no-op로 처리되고, `\@unexpandable@protect`는 current narrow scaffold에서 `\edef`/`\xdef` loop 안에서 `\protect`와 같은 noexpand alias로 처리되며, `\uppercase`와 `\lowercase`는 balanced-group 안의 character token만 ASCII 기준으로 변환하고 control sequence token은 그대로 두는 conservative text-transform helper로 같은 preamble-definition/capture 경로에 포함되며, `\strip@prefix`는 `\meaning`이 만든 `macro:->...` 스타일 prefix를 first-`>` delimiter까지 제거하는 narrow helper로 direct expansion과 `\edef` capture 양쪽에서 고정돼 있다.
* 같은 low-level token-list helper cluster에는 `\makeatletter`-scoped `\@tfor`도 포함된다. 현재 구현은 grouped token list를 item-by-item으로 순회하는 narrow semantics이고, wrapper class/package option stack 위의 dedicated corpus fixture로 later option-only revision까지 고정돼 있다.
* 같은 low-level loop helper cluster에는 `\makeatletter`-scoped `\@whilenum`, `\@whilesw`, 그리고 plain `\loop...\repeat`도 포함된다. 현재 구현은 `\@whilenum`/`\@whilesw`의 condition/body를 recursion-style로 reinject 하면서 numeric relation과 boolean switch를 좁게 반복 실행하고, plain `\loop...\repeat`도 top-level `\repeat` delimiter까지 body를 좁게 수집한 뒤 internal iterate macro로 reinject 하면서 trailing `\if...\else...\fi` pattern을 따라 반복 실행하는 semantics이며, wrapper class/package option stack 위의 dedicated corpus fixture로 later option-only revision까지 고정돼 있다.
* 같은 low-level list rebuild helper cluster에는 `\makeatletter`-scoped `\@cons`와 `\@removeelement`도 포함된다. 현재 구현은 `\@elt`-style list body를 global append하고 comma-separated token list에서 exact item을 좁게 제거하는 semantics이고, wrapper class/package option stack 위의 dedicated corpus fixture로 later option-only revision까지 고정돼 있다.
* 같은 low-level membership helper cluster에는 `\makeatletter`-scoped `\in@` / `\ifin@`도 포함된다. 현재 구현은 needle/haystack token list membership를 좁게 계산해 snapshot-aware boolean state로 유지하는 semantics이고, wrapper class/package option stack 위의 dedicated corpus fixture로 later option-only revision까지 고정돼 있다.
* 같은 low-level hook helper cluster에는 `\makeatletter`-scoped `\g@addto@macro`도 포함된다. 현재 구현은 existing macro body에 argument token list를 global append하고, wrapper class/package option stack 위의 dedicated corpus fixture로 group 안 append와 later option-only revision까지 고정돼 있다.
* 같은 low-level argument expansion helper cluster에는 `\makeatletter`-scoped `\@thirdofthree`와 `\@expandtwoargs`도 포함된다. 현재 구현은 third-argument grabber와 fully-expanded two-argument reinjection을 좁게 처리하고, wrapper class/package option stack 위의 dedicated corpus fixture로 later option-only revision까지 고정돼 있다.
* 같은 preamble boolean helper cluster에는 `\newif`와 builtin `\iftrue/\iffalse`, `\makeatletter`-scoped `\@fileswtrue/\@fileswfalse/\if@filesw`, plain `\nofiles`도 포함된다. 현재 구현은 package/class wrapper가 local option state나 aux-write enable state를 boolean conditional로 materialize 하는 narrow semantics이고, skipped branch 안의 user-defined conditional depth도 함께 추적해서 dedicated corpus fixture로 later option-only revision까지 고정돼 있다.
* 같은 scratch counter helper cluster에는 `\newcount`, `\countdef`, `\setcounter`, `\addtocounter`, `\stepcounter`, `\refstepcounter`, `\value`, public `\arabic/\roman/\Roman/\alph/\Alph`, `\makeatletter`-scoped `\@arabic/\@roman/\@Roman/\@alph/\@Alph`, 그리고 `\@addtoreset`도 포함된다. 현재 구현은 `\newcount\c@...` alias 위에서 common LaTeX counter update/read path, counter-format path, step/refstep-driven child-counter reset path를 좁게 수행하는 semantics이고, wrapper class/package option stack 위의 dedicated corpus fixture로 later option-only revision까지 고정돼 있다.
* 같은 scratch counter helper cluster에는 `\makeatletter`-scoped `\@removefromreset`도 포함된다. 현재 구현은 parent counter에 연결된 child reset relation을 snapshot-aware state에서 다시 제거하는 좁은 semantics이고, wrapper class/package option stack 위의 dedicated corpus fixture로 later option-only revision까지 고정돼 있다.
* 같은 scratch dimen/length helper cluster에는 `\newdimen`, `\dimendef`, plain `\dimen`, public `\newlength`, `\setlength`, `\addtolength`, builtin scratch dimen aliases(`\dimen@`, `\@tempdima`, `\@tempdimb`), 그리고 `\p@`도 포함된다. 현재 구현은 snapshot-aware dimen register alias 위에서 common LaTeX length update/read path와 `\the`의 `pt` materialization path를 좁게 수행하는 semantics이고, wrapper class/package option stack 위의 dedicated corpus fixture로 later option-only revision까지 고정돼 있다.
* 같은 scratch glue/skip helper cluster에는 `\newskip`, `\skipdef`, plain `\skip`, builtin scratch skip aliases(`\skip@`, `\@tempskipa`, `\@tempskipb`), 그리고 `\the`의 conservative `pt` rendering도 포함된다. 현재 구현은 snapshot-aware skip register alias 위에서 common LaTeX glue-state setup/readback path를 좁게 수행하고, grouped or bare `pt`/`sp` payload 뒤의 narrow `plus` / `minus` tails도 base value를 유지한 채 소비하는 semantics이며, wrapper class/package option stack 위의 dedicated corpus fixture로 later option-only revision까지 고정돼 있다.
* 이 corpus gate는 이제 fixture별 `EXPECT.txt`, optional `ABSENT.txt`, optional `EXECUTED-EXPECT.txt`, optional `ARTIFACT-EXPECT.txt`, optional `JSON-EXPECT.txt`, optional `FAIL-JSON-EXPECT.txt`를 읽어서, 기대 문자열이 실제 output에 나타나는지와 raw semantic/package command가 output이나 executed source에 새지 않는지, materialized executed source가 기대한 rewrite 결과를 담는지, revision artifact(`semantic.aux`, `semantic-index.json`, `build-meta.json` 등)가 실제로 생성되고 기대 payload나 structured JSON field를 담는지, 그리고 expected failure가 `stage` / `subject_kind` / `surface_kind` 기준에서도 맞는지를 함께 본다.
* 같은 structured failure coverage는 이제 current failure corpus 전반에 깔려 있다. larger split-preamble realistic workflow family뿐 아니라 optioned/diagnostic failure fixture까지 `REVN-FAIL.txt` 위에 `REVN-FAIL-JSON-EXPECT.txt`를 같이 두고, failure stage/subject/surface classification을 same corpus gate에서 직접 검증한다.
* fixture는 이제 optional `REVN-DELETE.txt`와 `REVN-FAIL.txt`도 지원해서, later revision에서 file deletion이나 expected compile failure까지 corpus 수준으로 검증할 수 있다. 같은 fixture 안에서 failed revision 뒤 다음 `revN/` overlay로 recovery success까지 이어서 검증할 수도 있다.
* fixture 아래 optional `revN/` overlay와 `REVN-*` expectation 파일을 두면, 같은 corpus gate가 multi-revision build도 다시 돌려서 semantic aux backdating, unchanged page reuse, semantic change rebuild, build-meta 같은 rebuild behavior까지 실제로 검증한다. 현재는 semantic-aux-heavy fixture와 `revtex`-style semantic stack fixture 둘 다 second-revision semantic-backdating과 third-revision semantic-change `.bbl` rebuild 경로를 타고, `semantic_aux_backdated=false`, semantic pass/rerun count, rebuilt/reused page metadata까지 함께 확인한다. 별도의 `missing-include-delete`, `missing-package-delete`, `missing-class-delete` fixture는 `REV2-DELETE.txt`/`REV2-FAIL.txt`로 missing input/package/class failure를, `rev3/` overlay로 그 다음 revision recovery success까지 같은 harness에서 고정한다. `minted-cache`와 `xelatex-font-filename` fixture는 delete/recover revision을 통해 local cache/font asset loss와 recovery도 같은 multi-revision path에서 본다.
* 같은 multi-revision path는 bibliography artifact semantics도 직접 검증한다. 별도의 bibliography-style / bibliography-input-order fixture는 visible output은 유지한 채 `\bibliographystyle{...}`나 `\bibliography{...}` input order만 바꾸고, `semantic.aux`의 `\bibstyle{...}` / `\bibdata{...}`와 `build-meta.json`이 그 semantic change를 실제로 기록하는지 확인한다.
* bibliography input loss도 corpus에 올라가 있다. 현재 semantic path에서는 `refs.bbl` 삭제가 hard failure가 아니라 unresolved citation text와 empty bibliography로 수렴하므로, 별도의 missing-bibliography fixture가 그 degraded output과 다음 revision recovery를 함께 고정한다.
* bibliography file이 없는 heading-only 경로도 corpus에 올라가 있다. 별도의 `printbibheading` fixture는 `\printbibheading[heading=bibintoc,title=...]`만 있는 문서에서 `semantic.aux`의 TOC write, raw-command stripping, 그리고 revision별 title change를 함께 검증하고, optioned variant는 같은 heading-only 경로가 wrapper class/package option stack 위에서도 revision별 title change와 option change를 함께 견디는지 본다.
* theorem declaration change도 corpus에 올라가 있다. 별도의 fixture는 `\newtheorem` counter scope가 revision마다 바뀔 때 visible theorem numbering, executed-source rewrite, 그리고 semantic label artifact가 같이 바뀌는지를 검증하고, optioned variant는 같은 theorem declaration change가 wrapper class/package option stack 위에서도 그대로 semantic dirty rebuild를 타는지 본다.
* float caption/list semantics도 corpus에 올라가 있다. 기존 semantic-aux-heavy fixture가 `\caption[Short]{Long}` + `\listoffigures`를 한 프로젝트에서 검증하고 있고, optioned variants는 같은 caption/list path와 `\captionof{figure}` path가 wrapper class/package option stack 위에서도 revision별 caption change와 later option change를 함께 견디는지 본다.
* short-title TOC semantics도 corpus에 올라가 있다. 기존 semantic-aux-heavy fixture가 `\section[Short]{Long}`를 한 프로젝트에서 검증하고 있고, optioned variant는 같은 short-title/long-title split이 wrapper class/package option stack 위에서도 revision별 title change와 later option change를 견디며 `semantic.aux`의 `\contentsline`까지 유지하는지 본다.
* split-document include filtering도 corpus에 올라가 있다. optioned variant는 wrapper class/package option stack 위에서 `\includeonly{...}` revision change가 TOC, visible output, executed include files, semantic labels를 함께 바꾸고, later option-only revision은 semantic aux를 backdate하는지 본다.
* manual TOC entry semantics도 corpus에 올라가 있다. optioned variant는 wrapper class/package option stack 위에서 `\phantomsection` / `\addcontentsline{toc}{section}{...}` title change와 later option change를 concrete `semantic.aux` `\contentsline` write, raw-command stripping, visible TOC/body output으로 함께 고정한다.
* appendix/reference semantics도 corpus에 올라가 있다. optioned variants는 wrapper class/package option stack 위에서 `\appendix` lettering, appendix TOC entry, `\autoref`, `\nameref`, `\appendices` synonym, 그리고 chapter-style appendix/autoref 경로까지 later option change와 함께 고정한다.
* theorem declaration semantics도 corpus에 올라가 있다. optioned variants는 wrapper class/package option stack 위에서 `\newtheorem` counter scope change뿐 아니라 `\theoremstyle`, `\newtheoremstyle`, `\swapnumbers` strip과 custom theorem shared-counter rewrite까지 later option change와 함께 고정한다.
* theorem multi-ref semantics도 corpus에 올라가 있다. optioned variant는 wrapper class/package option stack 위에서 repeated theorem labels가 `\cref`, `\namecrefs`, `\nameref`를 거쳐 pluralization/title-based ref까지 유지하는지 later option change와 함께 고정한다.
* equation/reference semantics도 corpus에 올라가 있다. optioned variant는 wrapper class/package option stack 위에서 equation labels가 `\eqref`, `\autoref`, `\cref`, `\namecref`, `\vref`, `\crefrange`를 거쳐 semantic label-set revision과 later option change까지 견디는지 concrete `semantic.aux` label artifact와 함께 고정한다.
* float/reference semantics도 corpus에 올라가 있다. optioned variant는 wrapper class/package option stack 위에서 figure/table/algorithm labels가 `\autoref`, `\cref`, `\namecref`, `\vref`를 거쳐 float-kind revision과 later option change까지 견디는지 concrete `semantic.aux` label artifact와 함께 고정한다.
* page-range reference semantics도 corpus에 올라가 있다. optioned variant는 wrapper class/package option stack 위에서 `\pagerefrange`, `\vpagerefrange`, `\vrefrange`, `\cpagerefrange`가 label numbering revision과 later option change까지 견디는지 concrete `semantic.aux` label artifact와 함께 고정한다.
* label-only reference semantics도 corpus에 올라가 있다. optioned variant는 wrapper class/package option stack 위에서 `\labelcref`와 `\labelcpageref`가 equation-number revision과 later option change까지 견디는지 concrete `semantic.aux` label artifact와 함께 고정한다.
* package/class option propagation도 corpus에 올라가 있다. 별도의 fixture들은 `\PassOptionsToPackage{draft|final}{shim}`, `\PassOptionsToClass{twocolumn|onecolumn}{article}`, `\documentclass[...]{wrapper}`가 `\LoadClassWithOptions`를 통해 base class로 옵션을 넘기는 경로, `\ProcessOptions` 뒤에 오는 `\LoadClassWithOptions{article}`가 wrapper class option을 nested class로 실제 전달하는 경로, 같은 wrapper class가 `\LoadClassWithOptions{article}`와 `\RequirePackageWithOptions{hyperref}`를 함께 써서 documentclass option을 nested class와 nested package 양쪽으로 동시에 전달하는 경로, `\RequirePackageWithOptions{hyperref}`가 wrapper package option을 nested package로 실제 전달하는 경로, `\DeclareOption*{\PassOptionsToPackage{\CurrentOption}{...}}`와 `\DeclareOption*{\PassOptionsToClass{\CurrentOption}{...}}`가 nested package/class로 unknown option을 넘기는 경로, 그리고 class wrapper가 known class option은 `\DeclareOption{twocolumn}{\PassOptionsToClass{\CurrentOption}{article}}`에서 base class로 넘기고 unknown option은 `\DeclareOption*{\PassOptionsToPackage{\CurrentOption}{hyperref}}`에서 nested package로 넘기는 조합 경로, `\DeclareOption{foo}{\PassOptionsToPackage{\CurrentOption}{...}}`가 declared-option body 안에서 `\CurrentOption`을 실제로 쓰는 경로, `\ExecuteOptions{draft}`가 package/class wrapper의 default option을 먼저 적용하고 explicit revision change가 이를 override하는 경로, class-side `\ExecuteOptions{onecolumn}`와 explicit `\documentclass[onecolumn]{wrapper}`가 겹쳐도 option body가 한 번만 실행되는 경로, default option과 explicit option이 같은 값을 반복해도 `\ProcessOptions`가 그 body를 한 번만 실행하는 경로, 그리고 common `\ProcessOptions*` variant를 모두 덮어서, option propagation path가 strip-only no-op가 아니라 실제로 동작하는지 검증한다.
* larger multi-file preamble realism도 corpus에 올라가 있다. dedicated `split-preamble-semantic-stack` fixture는 wrapper `wrapper.cls -> profile.cls -> article.cls` class chain, local `shim.sty -> shim.cfg -> shim.def` package/config chain, 그리고 `\input{preamble/setup}` -> `\input{preamble/core}` -> `\input{preamble/macros}` / `\input{preamble/theorems}` 로 이어지는 nested preamble split 위에서 local package chain(`hyperxmp`, `cleveref`), theorem declaration rewrite, bibliography materialization, TOC rewrite가 함께 도는지 보고, later preamble-file semantic change와 later option-only revision뿐 아니라 nested preamble input deletion/recovery, local package deletion/recovery, local class deletion/recovery, body include deletion/recovery까지 concrete artifacts로 고정한다.
* representative larger bibliography/paper-family fixture는 이제 `semantic-index.json`도 structured JSON으로 직접 핀다. `split-preamble-bibliography-workflow` / `split-preamble-paper-family-workflow`는 base shape와 bibliography input-order/style drift를, `split-preamble-biblatex-paper-family-workflow` / `split-preamble-manual-biblatex-paper-family-workflow`는 base shape와 bibliography input-order/manual-heading drift를 `JSON-EXPECT.txt`로 직접 고정한다.
* 그에 맞춰 local package/class/toolchain preamble에서 자주 나오는 `\NeedsTeXFormat`, `\ProvidesFile`, `\ProvidesPackage`, `\ProvidesClass`, `\PassOptionsToPackage`, `\PassOptionsToClass`, `\DeclareOption`, `\DeclareOption*`, `\ExecuteOptions`, `\ProcessOptions`, `\ProcessOptions*`, `\relax`, `\RequirePackage`, `\RequirePackageWithOptions`, `\LoadClass`, `\LoadClassWithOptions`, `\AtBeginDocument`, `\AtEndDocument`, `\AtEndOfPackage`, `\AtEndOfClass`, `\global`, `\long`, `\protected`, `\outer`, `\futurelet`, `\string`, `\protect`, `\meaning`, `\detokenize`, `\strip@prefix`, `\ignorespaces`, `\jobname`, `\makeatletter`-scoped `\@currname/\@currext/\@currpath`, `\filename@parse`, `\filename@area`, `\filename@base`, `\filename@ext`, `\newcommand`, `\renewcommand`, `\providecommand`, `\DeclareRobustCommand`와 그 starred forms, optional first-argument defaults(`[2][default]`), `\IfFileExists`, `\InputIfFileExists`, `\newread`, `\openin`, `\closein`, `\ifeof`, plain `\read`, plain `\readline`, `\newwrite`, `\openout`, `\closeout`, `\immediate`, `\write`, `\protected@write`, `\newtoks`, `\toksdef`, plain `\toks`, `\newdimen`, `\dimendef`, plain `\dimen`, public `\newlength`, `\setlength`, `\addtolength`, `\endinput`, `\unless`, `\aftergroup`, `\afterassignment`, `\advance`, `\multiply`, `\divide`, `\newcount`, `\countdef`, `\setcounter`, `\addtocounter`, `\stepcounter`, `\refstepcounter`, `\value`, `\the`, `\number`, `\romannumeral`, public `\arabic`, `\roman`, `\Roman`, `\alph`, `\Alph`, `\@arabic`, `\@roman`, `\@Roman`, `\@alph`, `\@Alph`, `\@addtoreset`, builtin `\space`, `\@backslashchar`, `\@percentchar`, `\@hashchar`, scratch count aliases(`\count@`, `\@tempcnta`, `\@tempcntb`), scratch dimen aliases(`\dimen@`, `\@tempdima`, `\@tempdimb`), numeric constant macros(`\z@`, `\@ne`, `\tw@`, `\thr@@`, `\m@ne`), and `\p@`의 좁은 scaffold가 internal VM/compiler 경로에 들어가 있다. 이 중 `\PassOptionsToPackage`, `\PassOptionsToClass`, `\DeclareOption`, `\DeclareOption*`, `\ExecuteOptions`, `\ProcessOptions`, `\ProcessOptions*`, `\RequirePackageWithOptions`, `\LoadClassWithOptions`는 이제 local package/class chain에서 narrow option propagation을 실제로 수행하고, `\CurrentOption`도 package/class wrapper의 default-option과 declared-option body 양쪽에서 좁게 해석하며, `\ProcessOptions` 이후에도 nested `\RequirePackageWithOptions`/`\LoadClassWithOptions`가 원래 전달된 option set을 계속 볼 수 있고, default와 explicit이 같은 option을 반복할 때는 `\ProcessOptions`가 body를 한 번만 실행한다. `\global`은 grouped definition helpers를 바깥 scope로 승격하는 narrow prefix로 처리되고, `\long`/`\protected`/`\outer`는 declaration prefix no-op로 처리되며, `\futurelet`은 looked-ahead token을 소비하지 않은 채 다음 helper로 넘기는 narrow lookahead primitive로, `\string`은 다음 control sequence name이나 character token을 확장 없이 그대로 출력하는 narrow token-stringifier로, `\protect`는 plain execution에서는 non-visible no-op로, `\edef`/`\xdef` full-expansion loop 안에서는 literal control sequence를 runtime까지 보존하는 `\noexpand` alias로 처리되며, `\meaning`은 primitive/macro/character/undefined token meaning을 conservative text로 materialize 하는 narrow inspector로, `\detokenize`는 balanced-group token list를 expansion 없이 conservative character payload로 materialize 하는 narrow detokenizer로, `\strip@prefix`는 `\meaning` output의 leading prefix를 first-`>` delimiter까지 제거하는 narrow helper로, `\ignorespaces`는 바로 뒤의 space token run만 한 번 소비하고 첫 non-space token은 그대로 두는 narrow whitespace-trimmer로, `\jobname`은 top-level entry source stem을 conservative token list로 materialize 하고 entry source가 없을 때만 `texput`으로 fallback 하는 narrow job-name helper로, `\@currname`/`\@currext`는 current active source frame의 stem/extension을 conservative token list로 materialize 하고 `\@currpath`는 그 parent path를 trailing slash와 함께 materialize 하는 narrow current-module helpers로, `\filename@parse`는 given path token list를 area/base/ext로 좁게 split 해서 `\filename@area`, `\filename@base`, `\filename@ext` macro에 채우는 narrow filename parser로, `\the`와 `\number`는 count register / scratch counter value뿐 아니라 token-register payload와 dimen/length payload도 conservative token list로 materialize 하는 narrow inspector로, `\newtoks`와 `\toksdef`는 stable token-register alias definition을 제공하고 plain `\toks` assignment는 grouped token list를 snapshot-aware register state에 저장하는 narrow token-register helpers로, public `\arabic`, `\roman`, `\Roman`, `\alph`, `\Alph`와 low-level `\@arabic`, `\@roman`, `\@Roman`, `\@alph`, `\@Alph`는 `\newcount\c@...`-style counter state와 count-like macro arguments를 common LaTeX counter text로 materialize 하는 narrow counter-format helpers로, `\@addtoreset`는 step/refstep-driven child-counter reset 관계를 snapshot-aware state에 저장하는 narrow counter-reset helper로, builtin `\space`, `\@backslashchar`, `\@percentchar`, `\@hashchar`는 common package-internal text/meta characters를 direct macro payload로 materialize 하는 narrow zero-arg helpers로, `\romannumeral`은 positive integer expression을 lowercase Roman numeral token list로 바꾸고 nonpositive value는 비우는 narrow numeric expander로, `\unless`는 다음 low-level conditional helper의 truth value를 one-shot으로 뒤집는 narrow conditional prefix로, `\aftergroup`는 현재 group이 닫힌 직후 pending token을 reinject 하는 narrow group-exit helper로, `\afterassignment`는 다음 narrow assignment primitive가 성공적으로 끝난 직후 one-shot token을 reinject 하는 assignment-complete helper로 처리된다. `\InputIfFileExists`는 optional config/asset input을 실제로 수행하면서 success/fallback branch를 함께 고르고, `\IfFileExists`/`\InputIfFileExists`는 normalized found path를 `\@filef@und`에 좁게 남기고 miss 때는 이를 비우며, `\newread`는 distinct read-stream alias macro를 좁게 정의하고 `\openin`/`\closein`은 stream plus filename payload를 non-visibly 소비하며 `\ifeof`는 unopened/missing/open/closed state를 snapshot-aware conditional로 읽고 plain `\read ... to ...`와 plain `\readline ... to ...`는 next line을 control sequence body로 narrow-materialize 하되 EOF에서는 target을 `\relax` meaning으로 내려놓고, `\newwrite`는 distinct stream alias macro를 좁게 정의하고 `\openout`/`\closeout`는 stream plus filename payload를 non-visibly 소비하며, plain `\write`는 aux-style stream/body pair를 non-visibly 소비하면서 write payload를 same full-expansion helper 위에서 좁게 전개하고 `\immediate`는 그 path 앞에서 narrow no-op prefix로 동작하며, `\protected@write`는 aux-style stream/prefix/body triple을 non-visibly 소비하면서 같은 write payload expansion path를 재사용하고, `\newtoks`는 distinct token-register alias macro를 좁게 정의하고 `\toksdef`는 explicit token-register alias를 바인딩하며 plain `\toks`는 grouped token list payload를 snapshot-aware token-register state에 저장하고 `\the`는 그 stored token list를 direct output과 `\edef` capture 양쪽에서 재사용하며, `\newdimen`과 `\newlength`는 stable dimen-register alias를 좁게 할당하고 `\dimendef`는 explicit dimen-register alias를 바인딩하며 plain `\dimen`, `\setlength`, `\addtolength`는 grouped or bare `pt`/`sp` payload를 snapshot-aware dimen state에 저장하고 `\the`는 그 stored dimen payload를 direct output과 `\edef` capture 양쪽에서 `pt` text로 재사용하고, `\endinput`은 current input/package/class file에서 trailing token을 더 읽지 않고 같은 module end marker로 바로 떨어지게 하며, `\ProvidesFile`/`\ProvidesPackage`/`\ProvidesClass`는 local cfg/package/class header에서 `\filedate` / `\fileversion` / `\fileinfo`도 좁게 채우고, `\AtEndDocument`는 balanced-group token hook을 snapshot-aware VM state에 쌓아 두었다가 main token queue가 비는 시점에 reinject 하며, `\AtEndOfPackage`/`\AtEndOfClass`는 active module frame에 balanced-group token hook을 쌓아 두었다가 module end marker 직전에 reinject 하며, scratch counter helper cluster는 `\advance`/`\multiply`/`\divide`뿐 아니라 `\newcount`/`\countdef`/`\setcounter`/`\addtocounter`/`\stepcounter`/`\refstepcounter`/`\value`/public counter-format helper/`\@addtoreset`까지 wrapper preamble에서 common numeric-state setup과 reset/readback을 later option-only revisions까지 유지하는 dedicated corpus fixture로, scratch dimen/length helper cluster는 `\newdimen`/`\dimendef`/plain `\dimen`/`\newlength`/`\setlength`/`\addtolength`/`\the`/`\p@`/scratch dimen aliases까지 같은 wrapper preamble에서 common length-state setup과 readback을 later option-only revisions까지 유지하는 dedicated corpus fixture로 고정돼 있다.
* 따라서 지금 상태는 “M12 완료 bar는 충족했고, 남는 건 wider engine-profile hardening, artifact-driven invalidation tightening, richer renderer/display-list ownership, stronger external editor integration 같은 post-M12 follow-on”이라고 보는 것이 맞다.

## Related Notes

- Contributor-oriented implementation sequencing and pitfalls:
  [`docs/contributor-notes.md`](./contributor-notes.md)

[1]: https://info.arxiv.org/help/faq/texlive.html "https://info.arxiv.org/help/faq/texlive.html"
[2]: https://tectonic-typesetting.github.io/book/latest/getting-started/first-document.html "https://tectonic-typesetting.github.io/book/latest/getting-started/first-document.html"
[3]: https://tectonic-typesetting.github.io/?utm_source=chatgpt.com "The Tectonic Typesetting System"
[4]: https://ghostscript.readthedocs.io/en/latest/API.html "https://ghostscript.readthedocs.io/en/latest/API.html"
[5]: https://rustc-dev-guide.rust-lang.org/queries/salsa.html "https://rustc-dev-guide.rust-lang.org/queries/salsa.html"
[6]: https://github.com/mozilla/pdf.js/ "https://github.com/mozilla/pdf.js/"
[7]: https://github.com/typst/typst/blob/main/docs/dev/architecture.md "https://github.com/typst/typst/blob/main/docs/dev/architecture.md"
[8]: https://myriad-dreamin.github.io/tinymist/feature/preview.html "https://myriad-dreamin.github.io/tinymist/feature/preview.html"
[9]: https://info.arxiv.org/help/00README.html "https://info.arxiv.org/help/00README.html"
[10]: https://tectonic-typesetting.github.io/book/latest/v2cli/compile.html "https://tectonic-typesetting.github.io/book/latest/v2cli/compile.html"
[11]: https://docs.rs/crate/tinymist-preview/latest "https://docs.rs/crate/tinymist-preview/latest"
[12]: https://info.arxiv.org/help/submit_tex.html "https://info.arxiv.org/help/submit_tex.html"
[13]: https://info.arxiv.org/help/bulk_data.html "https://info.arxiv.org/help/bulk_data.html"
