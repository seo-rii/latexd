# latexd Work Backlog

이 문서는 `latexd`의 남은 작업을 한 번에 보기 위한 backlog다.  
기준 문서는 [`README.md`](/home/seorii/dev/hancomac/latexd/README.md) 와 [`PROGRESS.md`](/home/seorii/dev/hancomac/latexd/PROGRESS.md) 이고, 여기서는 그 내용을 실행 가능한 작업 묶음으로 다시 정리한다. 더 자세한 `M12` 세부 체크리스트와 완료 기준은 [`docs/m12-checklist.md`](/home/seorii/dev/hancomac/latexd/docs/m12-checklist.md) 에 둔다.

## Status Key

- `Done`: 이미 구현/회귀가 있는 작업
- `Future`: 지금 당장 호환 목표는 아니지만 추후 목표로 적어 두는 작업

## Current Focus

- `M12 arXiv hardening`은 `2026-03-27` 기준으로 완료로 본다.
- `M11`은 이제 `M11.1`~`M11.4`, `M12`는 `M12.1`~`M12.6`으로 쪼개어 관리한다.
- 현재 backlog의 우선순위는 post-`M12` follow-on 정리다.
- 따라서 지금 실제 우선순위는:
  - wider engine-profile hardening
  - artifact-driven invalidation beyond the current narrow profile
  - richer renderer/display-list ownership
  - stronger external editor integration

## Milestone Split

### Completed

- `M11.1`: concrete semantic aux artifact
- `M11.2`: semantic equality, backdating, rerun-to-fixpoint
- `M11.3`: common natbib/biblatex/theorem/float/reference rewrite surface
- `M11.4`: checkpoint-seeded replay, observability, page/checkpoint reuse
- `M12.1`: wrapper-heavy corpus baseline
- `M12.2`: bibliography/toolchain realism
- `M12.3`: package/wrapper interaction + semantic artifact tightening
- `M12.4`: failure/recovery + structured artifact coverage
- `M12.5`: renderer/session hardening
- `M12.6`: sync/editor hardening

### Current Backlog Meaning

- 이 문서는 더 이상 `M12` 미완료 항목을 추적하지 않는다.
- 아래 backlog는 모두 post-`M12` follow-on 또는 future scope다.

## A. Compatibility / Hardening Backlog

### A1. Corpus Realism

- `Done`: 더 큰 multi-file preamble fixture 추가
  - local `cls/sty/cfg/def`가 여러 단계로 엮인 프로젝트
  - preamble file split + nested `\input` / `\include`
  - wrapper class/package 체인이 길어진 실제 문서형 fixture
- `Done`: 더 현실적인 bibliography workflow fixture 추가
  - multi-`.bbl` + heading-only bibliography + manual title rewrite가 함께 있는 문서
  - bibliography asset delete/recover뿐 아니라 aux artifact drift가 같이 있는 문서
  - style/order change + semantic-change + recovery가 한 fixture 안에 함께 있는 문서
- `Done`: 더 현실적인 theorem/appendix/float/reference 조합 fixture 추가
  - theorem + cleveref + appendix + float captions가 한 문서에 섞인 경우
  - short title + manual TOC + theorem/block title refs가 함께 있는 경우
- `Done`: 실제 arXiv-style project shapes 확대
  - 큰 root README/profile 조합
  - 여러 local package/class와 split document가 같이 있는 문서
  - option-heavy wrapper stack과 semantic aux-heavy body가 동시에 있는 문서
- `Future`: fixture를 더 실제 논문 묶음으로 확장
  - semantic-aux-heavy paper보다 더 큰 “paper family” fixture
  - 분야별 archetype fixture
  - recent larger additions now include `split-preamble-heading-theorem-paper-family-workflow`, `split-preamble-bibliography-heavy-paper-family-workflow`, `split-preamble-appendix-heavy-paper-family-workflow`, `split-preamble-package-wrapper-paper-family-workflow`, `split-preamble-appendix-reference-paper-family-workflow`, `split-preamble-manual-heading-reference-paper-family-workflow`, `split-preamble-citation-caption-reference-heavy-paper-family-workflow`, `split-preamble-biblatex-heading-reference-heavy-paper-family-workflow`, `split-preamble-manual-heading-caption-reference-heavy-paper-family-workflow`, and `split-preamble-varioref-reference-heavy-paper-family-workflow`
    - theorem-heavy paper
    - bibliography-heavy paper
    - appendix-heavy paper
    - package-wrapper-heavy paper
    - appendix-reference paper
    - manual-heading-reference paper
    - citation-caption-reference-heavy paper
    - biblatex-heading-reference-heavy paper
    - manual-heading-caption-reference-heavy paper
    - varioref-reference-heavy paper
    - bibliography-toolversion paper
    - package-interaction paper
    - failure-semantic-chain paper

### A2. Toolchain / Package Compatibility

- `Done`: package-specific reference semantics 확대
  - 현재 hard-coded surface를 넘어 package interaction regression 추가
  - `hyperref`, `cleveref`, `varioref`, theorem/appendix/float surfaces의 혼합 경로 확대
- `Done`: bibliography/tool-version semantics 심화
  - `semantic.aux` / `build-meta.json`에서 style/input metadata drift를 더 직접적으로 검증
  - bibliography tool-version 차이를 나타내는 artifact-level fixture 정리
- `Done`: macro/package-driven semantic discovery gap 축소
  - 현재 hard-coded command list 바깥의 common wrapper 패턴을 fixture로 먼저 포착
  - custom theorem / wrapper package / wrapper class가 semantic discovery에 미치는 영향 확대
- `Future`: concrete aux/toolchain artifact 경로 심화
  - bibliography style/input change와 semantic equality/backdating 관계를 더 엄격하게 모델링
  - package-specific semantic artifacts를 `semantic.aux` 또는 companion artifact로 노출할지 검토
- `Future`: wider engine-profile hardening
  - 현재 narrow `pdflatex` path 위주의 shim에서 더 실제적인 local toolchain profile로 확장
  - `xelatex` / `latex+dvips_ps2pdf` 쪽 narrow fixture 확대

### A3. Failure / Recovery / Stability

- `Done`: failure/recovery corpus 조합 확대
  - missing input/package/class/font/cache 외에 bibliography + semantic stack 혼합 recovery
  - failure 뒤 later semantic-change rebuild까지 이어지는 revision chain
- `Done`: structured artifact expectations 확대
  - `build-meta.json`, `semantic.aux`, `semantic-index.json`에서 중요한 fields를 corpus에서 더 직접적으로 고정
- `Future`: corpus diagnostics expectations 정교화
  - expected failure message뿐 아니라 failure kind/classification까지 고정
  - recovery revision에서 reused/rebuilt page metadata 변화까지 고정

## B. Renderer / Preview Hardening

### B1. Long-Lived Renderer Session

- `Done`: Phase 1 actor/session wrapper
  - same document root + renderer mode requests now share one render lane
  - current full-page cold miss path no longer constructs a fresh renderer path per miss
- `Done`: [`renderer-session-plan.md`](/home/seorii/dev/hancomac/latexd/docs/renderer-session-plan.md) Phase 1
- `Done`: Phase 2
  - basic attach/detach, attached-page lookup, attached revision metadata-set lookup, attached revision debug exposure, and actor-owned LRU + page-budget retention are in
  - broader revision lifecycle ownership is now mostly about longer mixed old/new incremental chains rather than fixed build-loop detach policy
  - revision attachment / detach
  - unchanged pages reused from older revision도 session에서 바로 render 가능하게 만들기
- `Done`: Phase 5
  - actor spawn/restart/attach/detach/evict/full-render/tile-render/prewarm/fallback counters, debug surface, recent revision/page keyed event ring, matching structured debug logs, coarse per-event durations, aggregate latency summaries, and attached-session budget context are in
  - renderer crash 시 cache/fallback degradation path와 longer-session observability를 더 넓게 다듬기
- `Done`: Phase 3
  - rectangle-request tile path has started in a narrow form
  - same tile request는 actor-owned `render_tiles`와 small rect-tile cache를 타고, mixed cached+uncached batch도 missing rect만 다시 렌더한다
  - rect-tile cache는 이제 global LRU budget 위에 per-page rect budget도 같이 가져서 one page의 rectangle churn이 다른 page tile ownership을 밀어내지 않게 됐다
  - tile cache eviction도 이제 debug surface에서 counter/event로 보여서 per-page/global rect ownership churn을 직접 추적할 수 있다
  - repeated identical tile hit는 actor debug surface에서 tile-cache count, concrete cached rect entry list, explicit `ReuseTiles` event로 구분해 볼 수 있다
  - broader rectangle batching/display-list ownership은 여전히 남아 있다
- `Done`: Phase 4
  - session-aware prewarm
  - viewport prewarm의 narrow actor-owned warm path, same revision/page/zoom inflight duplicate suppression, current-page neighboring zoom-step warmup은 들어갔고, actor-owned warm bucket retention/debug exposure도 이제 들어왔다
  - warm bucket은 이제 per-page zoom-bucket budget도 가져서 single-page zoom churn이 다른 warmed page state를 밀어내지 않게 됐다
  - 남은 건 longer-lived warm ownership tuning이다

### B2. Preview / Sync Fidelity

- `Done`: sync geometry fidelity 확대
  - 현재 line-aware sync box를 더 안정적인 box-level geometry로 확장
  - page-local source/output range drift regression과 item-level output window validation은 들어갔고, 남은 건 true box attribution에 더 가까운 geometry tightening이다
- `Done`: editor bridge 강화
  - source-jump/open-source bridge response가 nearest page/item/page-local source-output window와 page geometry, ready-to-use canonical source hash를 포함하게 됐고, viewer도 그 canonical hash를 current page/selected source, source link, URL history, resolved embedding event(`latexd:source-jump-resolved`, `latexd:source-hover-resolved`, `latexd:open-source-resolved`)로 소비하게 됐다
  - canonical `source_hash`도 이제 column이 있으면 `&column=`까지 유지하고, viewer request reconstruction도 그 canonical column을 다시 읽어서 source pane / local editor / preview roundtrip에서 line-only ambiguity를 줄인다
  - `window.latexdJumpToSource(...)` / `window.latexdSelectSource(...)` / `window.latexdHoverSource(...)`도 이제 `{ sourceHash }` / `{ source_hash }` 뿐 아니라 plain `"#src=..."` 입력을 직접 풀 수 있고, `window.latexdOpenSelectedSource(...)` / `window.latexdPreviewSelectedSource(...)`도 same canonical input을 직접 POST할 수 있다. `latexdOpenSelectedSource(...)`는 direct canonical input과 `launch: false`도 같은 imperative entrypoint에서 같이 받을 수 있다. `/api/source-jump/<rev>` 와 `/api/open-source/<rev>` 둘 다 `source_hash`-only request를 받아 nested-path/column selection을 한 번 더 roundtrip해도 재구성 오차를 줄이고, direct hash/object path detail도 resolved/failed event와 returned promise에서 같은 canonical `source` shape를 다시 내보낸다
  - sync item은 stable `item_id`를 같이 싣고, viewer는 syncmap refresh에서 같은 `item_id`가 남아 있으면 selection/hover를 새 geometry/output window로 다시 붙인다
  - `/api/source-jump/<rev>`와 `/api/open-source/<rev>`가 이제 둘 다 absolute file path, editor cwd, launch-support state, `file_uri`, `editor_uri`, materialized `editor_program` / `editor_args` / `editor_command_line` preview, `editor_preview_kind`(`none` / `uri` / `command` / `command_and_uri`)를 같은 shape로 돌려서 external shell/local editor handoff를 jump/open-source 어느 path에서도 같은 방식으로 제어할 수 있게 됐다
  - imperative `window.latexdJumpToSource(...)`, `window.latexdSelectSource(...)`, `window.latexdHoverSource(...)`도 이제 raw response 대신 same resolved detail payload를 promise로 돌려줘서, embedding shell이 event listener 없이도 canonical hash / page-item context / editor preview를 바로 재사용할 수 있다
  - `/api/open-source/<rev>` launch placeholder도 `rev/source_hash/page/item/output` context와 `{item_id}`, `{editor_cwd}`, `{absolute_file}`뿐 아니라 `{editor_program}`, `{editor_command_line}`, `{editor_preview_kind}`까지 직접 materialize 하게 됐고, `launch: false` preview-only path나 no-bridge normal request에서도 spawn 없이 resolved handoff payload를 먼저 받을 수 있게 됐다. built-in viewer의 explicit open-source action도 이제 `editorBridgeEnabled=false`에서 same resolved payload를 실제 fetch해서 `latexd:open-source-resolved`로 넘기고, `source-jump` / `source-hover` / `open-source` fetch failure는 `latexd:*‑failed` event로 분리해서 embedding shell이 preview fallback과 request failure를 구분할 수 있다
  - 남은 건 현재 DOM/hash/local command bridge를 넘는 stronger external editor integration이다
  - source pane / local editor / preview selection roundtrip 정교화

## C. Semantic / Compiler Refinement

### C1. Replay / Invalidation Tightening

- `Done`: semantic-changing bibliography input no longer reuses stale preamble prefixes
  - changed `.bbl`이 current bibliography set에 포함되고 semantic aux가 실제로 달라지면 final semantic build는 reused preamble checkpoint 대신 base snapshot에서 다시 돈다
  - semantically-equal later `.bbl` edits는 그대로 page/checkpoint reuse를 유지하고, semantic-changing `.bbl` edits만 conservative rebuild로 내려간다
- `Future`: semantic artifact 중심 invalidation 강화
  - raw source/executed source heuristic보다 semantic artifact drift를 더 직접 기준으로 삼기
  - backdating / rebuild / replay 선택을 더 구조화하기
- `Future`: semantic change와 page reuse 관계 정교화
  - currently rebuilt page count가 conservative한 경로 줄이기
  - semantic-equal revision에서 page/checkpoint reuse를 더 공격적으로 유지하기
- `Done`: larger paper-family recovery metadata drift 고정
  - `split-preamble-*paper-family-workflow` recovery rev가 `rebuilt_page_count` / `reused_page_count` / `semantic_pass_count`까지 structured JSON expectation으로 잠겨 있다

### C2. Observability

- `Done`: `build-meta.json` / `semantic-index.json` / `semantic.aux`와 `FAIL-JSON-EXPECT.txt`를 기준으로 실제 운영 판단에 필요한 field 정리
- `Done`: representative larger bibliography/paper-family fixture의 `semantic-index.json` root/file-summary drift 고정
  - `split-preamble-bibliography-workflow` / `split-preamble-paper-family-workflow`가 base shape + bibliography input-order/style drift를 structured JSON으로 직접 핀다
  - `split-preamble-biblatex-paper-family-workflow` / `split-preamble-manual-biblatex-paper-family-workflow`가 base shape + bibliography input-order/manual-heading drift를 structured JSON으로 직접 핀다
- `Done`: current failure corpus now has `FAIL-JSON-EXPECT.txt` coverage end-to-end
  - split-preamble realistic workflow family와 optioned/diagnostic failure fixtures까지 preamble/package/class/body/command/error failure chain의 structured failure classification이 corpus-level default가 됐다
- `Future`: corpus gate에서 structured JSON/artifact expectation coverage 확대
- `Future`: lightweight dashboard/debug view
  - revision별 semantic pass, page reuse, checkpoint start, dirty files를 쉽게 볼 수 있는 surface

## D. Future Goals

아래 항목들은 지금 당장 `M12` 호환 목표는 아니지만, 추후 목표로 적어 둔다.

### D1. Fuller TeX / Toolchain Compatibility

- `Future`: source-rewrite scaffold를 넘는 richer aux/toolchain pipeline
- `Future`: BibTeX/Biber process emulation 심화
- `Future`: macro-generated semantic discovery 확대
- `Future`: wider TeX Live/package compatibility
- `Future`: full `xelatex` / `latex+dvips_ps2pdf` profile hardening

### D2. Richer Rendering

- `Future`: true page scene-graph / custom page renderer
- `Future`: SVG/PDF hybrid preview를 넘는 richer render representation
- `Future`: box/glue/page-builder 수준의 actual layout-engine fidelity

### D3. Richer Source / Editor UX

- `Future`: true box-level source attribution
- `Future`: editor-native jump/highlight protocol
- `Future`: richer source map / click-jump / hover-sync surface

### D4. Non-Compatibility Advanced Features

- `Future`: stronger preview diff visualization
- `Future`: revision history / replay debugging UI
- `Future`: artifact explorer for `semantic.aux`, `semantic-index.json`, `build-meta.json`
- `Future`: performance dashboard and regression trend tracking

## Suggested Working Order

1. post-`M12` engine/profile expansion
2. post-`M12` artifact-driven invalidation tightening
3. richer renderer/display-list ownership
4. stronger external editor integration
5. post-compatibility advanced features
