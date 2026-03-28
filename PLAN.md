# Milestone Breakdown

이 문서는 너무 큰 milestone이던 `M11`, `M12`를 실제 완료 기준에 맞춰 `M11.x`, `M12.x`로 나눈 현재 기준표다.

기준 문서:
- [`README.md`](/home/seorii/dev/hancomac/latexd/README.md)
- [`PROGRESS.md`](/home/seorii/dev/hancomac/latexd/PROGRESS.md)
- [`docs/m12-checklist.md`](/home/seorii/dev/hancomac/latexd/docs/m12-checklist.md)
- [`docs/work-backlog.md`](/home/seorii/dev/hancomac/latexd/docs/work-backlog.md)
- [`docs/renderer-session-plan.md`](/home/seorii/dev/hancomac/latexd/docs/renderer-session-plan.md)

## Status Snapshot

- 기준 시점: `2026-03-27`
- `M11`: `M11.1`~`M11.4` 완료
- `M12`: `M12.1`~`M12.6` 완료
- 현재 집중 범위: post-`M12` follow-on

## M11 Split

### M11.1 Concrete Semantic Aux Artifacts

상태: `completed`

실제 기준:
- concrete `semantic.aux`가 revision마다 persisted된다.
- labels, citations, bibliography, TOC surface가 artifact로 round-trip 된다.

### M11.2 Semantic Equality And Backdating

상태: `completed`

실제 기준:
- semantic equality가 raw source equality와 분리돼 있다.
- semantic-equal rebuild는 backdating과 bounded rerun-to-fixpoint를 탄다.

### M11.3 Executed-Source Rewrite Surface

상태: `completed`

실제 기준:
- natbib/biblatex, theorem, float/list, reference surface가 executed source에 raw command를 남기지 않는다.

### M11.4 Replay, Checkpoints, Observability

상태: `completed`

실제 기준:
- semantic replay가 checkpoint/page reuse와 연결돼 있다.
- `build-meta.json`, `semantic-index.json`, executed-source snapshot으로 rerun/replay 판단을 관찰할 수 있다.

## M12 Split

### M12.1 Wrapper-Heavy Corpus Baseline

상태: `completed`

실제 기준:
- split preamble + wrapper-heavy `cls/sty/cfg/def` project shape가 regression corpus에 올라가 있다.
- local preamble/package/class/body failure-recovery chain이 corpus로 고정돼 있다.

### M12.2 Bibliography And Toolchain Realism

상태: `completed`

실제 기준:
- bibliography order/style/tool-version drift, semantically-equal `.bbl`, semantic-change `.bbl`, partial `.bbl` loss/recovery가 regression으로 고정돼 있다.
- natbib/biblatex bibliography surface가 larger family corpus에 올라가 있다.

### M12.3 Package Interaction And Semantic Artifact Tightening

상태: `completed`

실제 기준:
- wrapper package/class interaction, option propagation, semantic discovery drift가 regression으로 고정돼 있다.
- `semantic.aux`, `build-meta.json`, `semantic-index.json`이 representative large family에서 structured expectation을 가진다.

### M12.4 Failure, Recovery, Structured Expectations

상태: `completed`

실제 기준:
- success/backdating/change/failure/recovery가 one corpus harness에서 multi-revision으로 검증된다.
- failure path는 `FAIL-JSON-EXPECT.txt`, recovery path는 JSON metadata expectation까지 포함한다.

### M12.5 Renderer Session Hardening

상태: `completed`

실제 기준:
- actor-owned renderer session, attached revision retention, tile-native path, prewarm, warm-bucket retention, metrics/debug surface가 들어가 있다.

### M12.6 Sync And Editor Hardening

상태: `completed`

실제 기준:
- page-local source/output window, stable `item_id`, canonical `source_hash`, normalized geometry, richer `/api/open-source` payload가 들어가 있다.
- viewer는 stale selection clearing와 same-item re-anchoring을 지원한다.

## Post-M12 Serial Order

1. wider engine-profile hardening
2. artifact-driven invalidation beyond the current narrow profile
3. richer renderer/display-list ownership
4. stronger external editor integration
5. non-compatibility advanced features

## Parallelization Lanes

### Lane A: Corpus-Only Fixture Expansion

병렬 가능: `yes`

소유 파일:
- [`fixtures/arxiv-smoke`](/home/seorii/dev/hancomac/latexd/fixtures/arxiv-smoke)

적합한 작업:
- new larger paper-family fixture
- existing family expectation widening
- structured `JSON-EXPECT` / `FAIL-JSON-EXPECT` coverage

### Lane B: Toolchain Regression And Shim Tightening

병렬 가능: `yes`

소유 파일:
- [`crates/tex-vm/src/lib.rs`](/home/seorii/dev/hancomac/latexd/crates/tex-vm/src/lib.rs)
- [`crates/tex-aux/src/lib.rs`](/home/seorii/dev/hancomac/latexd/crates/tex-aux/src/lib.rs)

적합한 작업:
- wrapper/package interaction
- artifact-driven invalidation
- engine/profile-specific semantic drift

### Lane C: Renderer Session

병렬 가능: `limited`

소유 파일:
- [`crates/latexd/src/lib.rs`](/home/seorii/dev/hancomac/latexd/crates/latexd/src/lib.rs)
- [`docs/renderer-session-plan.md`](/home/seorii/dev/hancomac/latexd/docs/renderer-session-plan.md)

적합한 작업:
- display-list ownership
- rectangle batching
- longer-lived warm/revision lifecycle

### Lane D: Viewer / Sync UX

병렬 가능: `limited`

소유 파일:
- [`web/viewer/app.mjs`](/home/seorii/dev/hancomac/latexd/web/viewer/app.mjs)
- [`web/viewer/app.test.mjs`](/home/seorii/dev/hancomac/latexd/web/viewer/app.test.mjs)
- [`crates/latexd/src/lib.rs`](/home/seorii/dev/hancomac/latexd/crates/latexd/src/lib.rs)

적합한 작업:
- editor roundtrip tightening
- stronger source/preview selection identity
- preview/source UX polish

### Lane E: Docs / Status Consolidation

병렬 가능: `yes`

소유 파일:
- [`README.md`](/home/seorii/dev/hancomac/latexd/README.md)
- [`PROGRESS.md`](/home/seorii/dev/hancomac/latexd/PROGRESS.md)
- [`docs/m12-checklist.md`](/home/seorii/dev/hancomac/latexd/docs/m12-checklist.md)
- [`docs/work-backlog.md`](/home/seorii/dev/hancomac/latexd/docs/work-backlog.md)

적합한 작업:
- status sync
- milestone split maintenance
- post-`M12` scope clarification
