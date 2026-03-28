# M12 Subphase Checklist

이 문서는 큰 milestone이던 `M12 arXiv hardening`을 실제 완료 기준에 맞춰 `M12.1`~`M12.6`으로 나눈 체크리스트다.

기준 문서:
- [`README.md`](/home/seorii/dev/hancomac/latexd/README.md)
- [`PROGRESS.md`](/home/seorii/dev/hancomac/latexd/PROGRESS.md)
- [`docs/work-backlog.md`](/home/seorii/dev/hancomac/latexd/docs/work-backlog.md)
- [`docs/renderer-session-plan.md`](/home/seorii/dev/hancomac/latexd/docs/renderer-session-plan.md)

## Goal

`M12`의 목표는 “대부분의 arXiv 논문”에 가까워지는 것이다.  
이 단계는 다음 네 축을 함께 끌어올리는 hardening milestone이다.

- corpus realism
- deeper toolchain/package semantics
- renderer/session hardening
- richer source-preview sync

## Status Key

- `[done]`: `M12` 완료 범위에 포함됐고 regression/구현이 있다
- `[future]`: `M12` 뒤의 post-milestone 목표다

## Completion Update

`2026-03-27` 기준으로 `M12.1`~`M12.6`은 모두 완료 상태로 본다.

## M12.1 Wrapper-Heavy Corpus Baseline

- `[done]` split preamble + wrapper option stack fixture families
- `[done]` local `wrapper.cls -> profile.cls -> article.cls` class chain
- `[done]` local `shim.sty -> shim.cfg -> shim.def` package/config chain
- `[done]` nested `preamble/setup -> core -> macros` and `-> theorems` chain
- `[done]` multi-revision failure/recovery for preamble/package/class/body delete/recover
- `[done]` representative wrapper-heavy project shape가 larger paper-family corpus로 올라가 있다

## M12.2 Bibliography And Mixed-Paper Realism

- `[done]` multi-`.bbl` order drift / `\bibstyle` drift / semantically-equal `.bbl` / semantic-change `.bbl`
- `[done]` partial `.bbl` loss/recovery
- `[done]` natbib/biblatex metadata/date/identifier/generic field workflow fixtures
- `[done]` heading-only bibliography and manual heading/title rewrite fixtures
- `[done]` mixed manual TOC + bibliography heading + float/theorem/reference combinations를 larger corpus에 합쳤다
- `[done]` theorem-heavy / bibliography-heavy / appendix-heavy / package-wrapper-heavy / reference-heavy / varioref-heavy archetype families를 regression corpus로 고정했다
- `[future]` 더 큰 분야별 archetype corpus를 실제 arXiv layout 수준으로 더 키우기

## M12.3 Package Interaction And Semantic Artifact Tightening

- `[done]` grouped `\usepackage`, `\RequirePackage`, `\RequirePackageWithOptions`
- `[done]` `\LoadClass`, `\LoadClassWithOptions`, `\PassOptionsToPackage`, `\PassOptionsToClass`
- `[done]` `\DeclareOption`, `\DeclareOption*`, `\ExecuteOptions`, `\ProcessOptions`, `\ProcessOptions*`
- `[done]` option forwarding after `\ProcessOptions`
- `[done]` `\CurrentOption` forwarding in declared/default option bodies
- `[done]` `\AtBeginDocument`, `\AtEndOfPackage`, `\AtEndOfClass`, `\AtEndDocument`
- `[done]` `\InputIfFileExists`, `\IfFileExists`, package/class loaded/version guards
- `[done]` low-level `\makeatletter` helper cluster large subset
- `[done]` `hyperref + cleveref + varioref + theorem/appendix/float/reference` interaction regression
- `[done]` bibliography tool-version/style/input drift를 artifact-level expectation으로 고정
- `[done]` representative large family에서 `semantic.aux`, `build-meta.json`, `semantic-index.json` structured expectations를 고정
- `[future]` wider engine-profile hardening
- `[future]` fuller BibTeX/Biber process emulation

## M12.4 Failure, Recovery, Structured Expectations

- `[done]` missing input/package/class/font/cache delete/recover chains
- `[done]` bibliography partial loss/recovery chains
- `[done]` semantic backdating vs semantic-change rebuild chains
- `[done]` failure 뒤 later semantic-change rebuild까지 이어지는 longer chains를 larger paper families에 넓혔다
- `[done]` `EXPECT.txt`, `ABSENT.txt`, `EXECUTED-EXPECT.txt`, `ARTIFACT-EXPECT.txt`, `JSON-EXPECT.txt`, `FAIL-JSON-EXPECT.txt`
- `[done]` `REVN-*`, `REVN-DELETE.txt`, `REVN-FAIL.txt`, `REVN-FAIL-JSON-EXPECT.txt`
- `[done]` `semantic_aux_backdated`, `semantic_pass_count`, `reused_page_count`, `rebuilt_page_count`, `dirty_files`를 structured JSON으로 고정
- `[done]` current failure corpus의 structured failure classification coverage를 complete 상태로 맞췄다
- `[future]` same-family fixture 안에서 more conservative page reuse/rebuild metadata expectations를 더 넓히기

## M12.5 Renderer / Session Hardening

- `[done]` Phase 1 actor/session wrapper
- `[done]` Phase 2 attached revision metadata-set lookup, LRU, page-budget retention
- `[done]` Phase 3 tile-native request path
- `[done]` Phase 4 actor-owned prewarm, in-flight dedupe, neighboring zoom-step warmup, warm-bucket retention/debug exposure
- `[done]` Phase 5 counters, event ring, structured debug logs, aggregate latency summaries
- `[future]` broader rectangle batching and longer-lived display-list ownership
- `[future]` broader latency/regression observability and longer-session degradation analysis

## M12.6 Sync / Editor Hardening

- `[done]` page-local source/output range와 sync box drift regression
- `[done]` item-level `output_start_utf8` / `output_end_utf8`
- `[done]` stable `item_id`
- `[done]` canonical `source_hash`
- `[done]` normalized geometry and stale-selection clearing
- `[done]` same-item re-anchoring on sync refresh
- `[done]` richer `/api/source-jump` / `/api/open-source` payload
- `[done]` materialized editor command preview and `launch: false` preview-only path
- `[future]` stronger external local-editor integration and preview/source selection roundtrip hardening

## Post-M12 Goals

- `[future]` wider engine/profile expansion beyond the current narrow `pdflatex`-first profile
- `[future]` artifact-driven invalidation beyond the current replay heuristic
- `[future]` richer renderer/display-list ownership
- `[future]` stronger external editor integration
- `[future]` non-compatibility advanced features such as artifact explorer, revision-history UI, preview diff, performance dashboard
