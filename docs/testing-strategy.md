# Testing Strategy

This document describes the test layers, fixture corpus, and performance metrics that pin
`latexd` behavior.

## Revision Correctness Regressions

The September 2026 review follow-up adds focused native and viewer contracts:

| Test | Evidence |
| --- | --- |
| `artifact_lifetime` | 20 actual internal edits with a stable tail and GC; fresh compiler drivers; cold artifact HTTP lookups; missing PDF/SVG recovery; eventual unreferenced-owner collection |
| `input_revision` | 100 controlled external-compiler save races, with a second input read after the edit; only final revisions publish; captured hashes/source text; native multi-file continuation and clean-build equivalence |
| `preview_revision` | Unpublished successful preamble change, failed follow-up, then undo and body edit reuse the published baseline and match a clean build |
| `project_revision::tests` | Source/asset isolation, symlink materialization, cycle rejection, and recorded live absolute input rejection |
| `raster_spec_tests` | Follow manifest URLs through the actual router and decode PNGs for tile sizes 128/256/512 and scales 1.234/1.236/0.104; grid edges, cache normalization, invalid inputs and budgets |
| Viewer core | Applied-revision tile/source acceptance during pending/failed builds; stale response rejection; inserted/deleted/reordered/replaced page selection |

Run native coverage with `cargo test -p latexd --lib`; filter by the names above
for focused work. Run viewer coverage and type generation from `web` with
`pnpm --filter @latexd/viewer-core test` and
`pnpm --filter @latexd/viewer-core build`. The fake external compiler in the
100-race test intentionally writes its input as the output artifact to isolate
daemon revision behavior; it is not a TeX fidelity test. Native clean comparisons
separately cover Document IR, page display lists, source maps and semantic aux.

Cold compiler/HTTP reconstruction checks disk ownership without an in-memory
cache. `preview_persistence_real_build_failure_restart_serves_pdf_tiles_and_frozen_source`
now also rebuilds daemon state after success and failure, restores the published
manifest, and serves the same deferred PDF, tiles and captured source. Lease
regressions cover connected previews, queued/running render and export jobs,
release and collection. Viewer commit tests reject mismatched patch bases and
fetch current state after reconnect or a resynchronization response.

The native suite additionally checks production shipout label pages with a
long display-math fixture whose pagination differs from legacy text layout.
External-PDF tests retain native page content/resources, inherited geometry and
optional-content catalog state. A locally installed `pdflatex` integration is
explicitly ignored by default and must be run separately. It is not a frozen
TeX Live compatibility gate.

Renderer tests compare separately requested tiles with a full Ghostscript
raster, check a single backend invocation across tile misses, and verify
fractional A4 geometry through the final edge pixel. Chromium checks Worker
responsiveness, termination/recovery, preview retention, page position and
asset URL lifetime. Broad arXiv fidelity, arbitrary engine continuation,
renderer-process crash recovery and edit-to-visible performance remain separate
gates.

## Seeded Native Mutation Equivalence

[`mutation_differential_tests.rs`](../crates/latexd/src/mutation_differential_tests.rs)
compares the eager checkpoint `CompilerDriver::compile_for_preview` path with a clean
project and empty build directory recreated for every edit. The preview side
retains its revision chain and only advances its applied revision on success.
Both sides retain eager debug/PDF artifacts to expose the complete oracle;
timings describe these compiler invocations, not the daemon's deferred export
mode or browser edit-to-visible latency. The daemon's explicit live-preview
policy below executes cold and reuses output pages; it is a separate mode.
The default test runs 32 edits; the ignored test defaults to 1,000 edits with
seed `0x4c4154455844`.

Each 16-edit cycle exercises middle/tail word edits, comments that move source
locations without changing paint, a macro in an included preamble file, body
input changes, paragraph and math edits, a deliberate error followed by repair,
undo, matching label/reference renames, page insertion/deletion, and a no-op
compile request. The last item measures the compiler driver; it does not claim
the daemon's unchanged-save event skips execution. The insertion adds 32 display
fractions; assertions require the actual renderer page count to increase and
then decrease on deletion. Comment-only edits must keep clean paint unchanged.

The oracle compares parsed RenderEvents and Document IR, output text and readset,
ordered page draw operations and identities, semantic/concrete aux, source maps,
display-list provenance, consumed source text, and diagnostics. JSON object key
order and independently allocated absolute project mounts are normalized.
Source offsets, array order, glyph positions, labels, and resource identities
remain exact. Paint is also compared with provenance separated out, while the
complete display list remains part of the interaction comparison. PDF object
ordering is not an oracle; active PDF artifact references must resolve to PDFs.

Each report separates `supported_approximate`, deliberate `expected_failure`,
unexpected `new_failure`, and `reference_unbuildable` results. The reference in
this harness is latexd's independent clean run. These are synthetic equivalence
results, not arXiv coverage or comparison with a mature TeX engine. No finding
allowlist turns a mismatch into a pass. The report copies each side's real build
work trace when available; missing counters remain unmeasured, and output-page
reuse is never treated as skipped VM execution.
The report includes compiler-only p50/p95 wall time and separately summed
measured counters for successful incremental edits and independent clean builds.
Unmeasured execution-skip counts stay null. These figures include oracle artifact
generation and are not edit-to-visible measurements.

Run the focused test with:

```bash
cargo test -p latexd --lib mutation_differential_supported_edits_match_independent_clean
```

Run the longer test with private background logging and retained evidence:

```bash
install -d -m 700 "$HOME/logs"
umask 077
mutation_run=$(mktemp -d "$HOME/logs/latexd-mutations.XXXXXX")
(
  CARGO_BUILD_JOBS=2 \
  LATEXD_MUTATIONS=1000 \
  LATEXD_MUTATION_SEED=0x4c4154455844 \
  LATEXD_MUTATION_OUTPUT="$mutation_run/evidence" \
    cargo test --release -p latexd --lib mutation_differential_thousand_edits_match_independent_clean \
      -- --ignored --nocapture
  mutation_exit=$?
  printf '%s\n' "$mutation_exit" > "$mutation_run/exit-status"
) > "$mutation_run/run.log" 2>&1 &
mutation_pid=$!
printf 'pid=%s run=%s\n' "$mutation_pid" "$mutation_run"
```

`LATEXD_MUTATIONS` and `LATEXD_MUTATION_SEED` configure the ignored test. A mismatch
stops the sequence immediately, fails the test, and preserves both projects,
checkpoints, outputs, normalized observations, the seed, the current input and
the report's first differing JSON path for each layer. Failure evidence is kept
even without `LATEXD_MUTATION_OUTPUT`; that option also retains successful runs.
After successful comparisons, the clean project is deleted and the production
revision collector retains its recent manifests and their artifact owners. A
real preview lease pins the last-good revision across deliberate errors. Active
PDF hashes are checked before and after collection, and peak workspace bytes
are recorded. This bounds successful-run artifacts without breaking live refs.
Keep the seed and run through the reported failing step to reproduce a failure.
Inspect the bounded log tail only after the exit-status file exists.

### Measured Run: 2026-09-08

The release run with seed `0x4c4154455844` completed all 1,000 mutations plus the
initial cold build: **1,001 paired attempts** (2,002 compiler invocations), with
938 successful `supported_approximate` pairs, 63 deliberate `expected_failure`
pairs, and **zero semantic, paint, aux, or interaction mismatches**. The focused
32-edit test also passed. The fixture produced one to three renderer pages and
exercised 62 actual page insertions and 62 deletions. These are synthetic tests of
the internal mini-kernel profile, not arXiv compatibility measurements.

Both sides generated eager debug artifacts and full PDFs for the oracle. The
runner used x86-64 Linux, an Intel Xeon CPU at 2.20 GHz, and Rust 1.94.0. Other
repository validation shared the runner, so wall times describe this run and
do not establish an isolated performance baseline or edit-to-visible latency.
The complete test took 1,224.34 seconds, excluding Cargo compilation.

| Successful builds | Count | Compiler p50 | Compiler p95 | Dispatched tokens | VM executions | Reused output pages |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Incremental edits, excluding initial cold | 937 | 403.794 ms | 1,562.334 ms | 1,454,830 | 4,684 | 187 |
| Independent clean, including initial cold | 938 | 379.449 ms | 1,518.585 ms | 1,693,496 | 6,566 | 0 |

`skipped_execution_tokens` remained unmeasured (`null`) on both sides. The
smaller dispatched-token total did not produce an overall wall-time improvement
in this run. The 62 no-op *driver requests* still dispatched 60,140 tokens in
total; their incremental p50/p95 were 263.948/954.406 ms. They do not measure the
daemon's separate unchanged-save elision.

Production revision collection and a real last-good preview lease ran throughout
the sequence. Every active page PDF remained present and unchanged before and
after collection. The workspace peaked at 22,312,328 bytes (21.28 MiB) before GC
and ended with eight retained revision directories, `rev-994` through `rev-1001`.
The complete report and tested-binary identity are retained locally under
`~/logs/latexd-mutation-thousand-1788841147814828196/`: respectively
`evidence/latexd-mutations-DTt2Ru/report.json` and `execution-environment.json`.
`run.log` records the successful test and `exit-status` is `0`.

### Live Preview Policy

`CompilerDriver::with_preview_artifacts()` executes the current semantic input
cold and avoids loading, recapturing, building, or serializing durable VM
checkpoints. Page PDF/SVG reuse reads the exact applied preview's published
metadata independently of execution state, including revision gaps. Current
display lists, source maps, source snapshots and aux are regenerated. A durable
policy marker stops a later eager compile from silently selecting an older
checkpoint before the live revision. Default CLI/oracle callers retain the
eager checkpoint path used by the 1,000-edit report above.

The explicit trace reason is `live_preview_cold_execution_output_reuse`, with
`checkpoint_write_outcome.status=not_attempted`, zero checkpoint attachments,
no restored prefix, and unmeasured skipped-token counts. Output-page reuse is
still counted separately. The eager run's dominant measured cost was checkpoint
selection/recapture/persistence (521.568 seconds across 937 successful edits),
versus 24.253 seconds in primary execution/aux and under six seconds in
debug/full-PDF export. Deferring PDF alone did not address that cost.

[`live_preview_policy_tests.rs`](../crates/latexd/src/live_preview_policy_tests.rs)
checks 20 source-location edits against independent eager clean builds, exact
display/provenance/aux/source equality, real revision GC and retained page
owners, missing SVG regeneration, failed and unpublished revision gaps, corrupt
old caches, and an eager → live → eager transition with a changed preamble and
aux. The three focused tests pass. Its ignored
`live_preview_policy_release_measurement` test performs 20 matched build pairs
and writes raw compiler traces when `LATEXD_LIVE_POLICY_REPORT` is set. It uses
the same output-equivalence checks and makes no wall-time assertion.

The 2026-09-08 release measurement passed all 20 matched pairs with zero output
differences. The same shared Linux/Rust runner described above was in use; this
is a small single-page fixture and a short policy comparison.

| Compiler mode, 20 builds each | p50 | p95 | First build | VM executions | Dispatched tokens | Reused output pages |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Live cold execution and page reuse | 27.319 ms | 40.687 ms | 643.256 ms | 100 | 23,860 | 13 |
| Independent eager clean | 782.983 ms | 986.121 ms | 729.598 ms | 140 | 26,480 | 0 |

Primary execution/aux work remained comparable: 444.301 ms aggregate in live
mode and 466.150 ms in eager mode. The existing
`checkpoint_reexecution_and_state_persistence` phase fell from 16,014.535 ms
to 28.178 ms. Its live-mode time is **source, aux, and page metadata storage**;
the compatible phase label does not imply checkpoint capture or serialization.
Every live build reported checkpoint persistence `not_attempted` and zero
attachments. Skipped semantic execution remains unmeasured (`null`), and the
first live build's 643.256 ms excludes any claim that every build meets a
250 ms target. These compiler traces do not include browser display latency.

The test itself took 18.30 seconds, excluding Cargo compilation. Raw traces are
retained at
`~/logs/latexd-live-policy-measurement-1788845086666791702/report.json`; sibling
`run.log` and `exit-status` record success with status `0`.

### Two Real-Paper Policy Measurements

The ignored `live_preview_policy_cc0_release_measurement` test uses two locked
CC0 corpus projects with all their source files and assets. Each project gets a
first live build, a source-identical live request using its published manifest,
and an independent eager clean build. Both live results must match the clean
display list including provenance, full sources, syncmap, aux, semantic index,
output text, diagnostics, and dependency readset. Page ownership is checked
separately, including every reused PDF/SVG reference. The four comparisons
passed with zero mismatches on 2026-09-08.

| Paper | Internal pages | First live call | Same-source live call | Eager clean | Reused pages on second call |
| --- | ---: | ---: | ---: | ---: | ---: |
| 2508.10038v1 | 15 | 2.452 s | 1.925 s | 18.839 s | 15 / 15 |
| 2404.05196v2 | 10 | 5.469 s | 5.036 s | 13.046 s | 10 / 10 |

These are single compiler-call samples on the shared runner, not p95 estimates,
local-edit measurements, browser latency, or pdfLaTeX comparisons. The first
call has a fresh project/output directory; the two papers share a process, so
font/runtime caches can carry over. Source-identical requests bypass the
daemon's no-op-save filter and still execute semantics cold: respectively
143,722 tokens over seven VM runs and 94,720 tokens over five runs, unchanged
between the two live calls. Page reuse reduces page export time, while execution
and layout still cost seconds. The 250 ms target is not achieved by this run.

The eager checkpoint/state-persistence phase consumed 16.676 and 7.992 seconds;
the corresponding live metadata writes took 19–22 ms without checkpoint
creation. Live full-PDF/debug export counts remained zero. Manifest publication
was measured separately at 10.2–14.2 ms per request. Internal results remain
`supported_approximate`; equality with this engine's clean output is not an
arXiv fidelity score.

The test uses the existing corpus-oracle build recipe: a recorded
`00README.yaml` override selects `pdf_latex` and the locked entrypoint because
arXiv's `00README.json` uses a different schema. Source archive and reference PDF
hashes are verified, and the report records all actual input hashes, including
that generated configuration. Paper source/assets are unchanged.

The release test passed in 49.91 seconds, excluding compilation, with exit
status `0`. Raw traces, exact comparisons, the executed binary identity and
input recipe are retained in
`.codex/live-cc0-phase-measurement/1788849864075908363/`; the private runner log is
`~/logs/latexd-live-cc0-phase-1788849864076069560.log`. Reproduction requires the
fetched corpus path in `LATEXD_LIVE_CC0_CORPUS` and a nonexistent evidence output
path in `LATEXD_LIVE_CC0_EVIDENCE`; the test downloads no inputs.

### Published Live Preview Mutation Oracle

The separate live mutation mode uses `with_preview_artifacts()` against an
independent eager clean build on every edit. Each successful result is committed
through the real published-preview manifest, and the following build must read
that exact applied revision. Intentional failures preserve the manifest bytes.
Every production GC pass is followed by durable recovery and validation of the
original page/artifact owners. The default test covers 32 edits; the ignored
`mutation_differential_live_thousand_edits_match_eager_clean` gate defaults to
1,000 edits and writes `live-preview-report.json` when
`LATEXD_LIVE_MUTATION_OUTPUT` selects an evidence parent directory.

This contract compares exact production display lists and provenance, syncmaps,
the full saved sources and execution origins, aux, output text, dependency
readsets, diagnostics, and errors. It also requires absent debug/full-PDF and
checkpoint files, a valid deferred export recipe, and the explicit cold-preview
trace. RenderEvent and Document IR debug JSON are intentionally absent from
live output and are **not compared** by this gate; the original eager oracle
retains its full semantic comparison. Neither mode is an arXiv fidelity score.

The 2026-09-08 release run completed all 1,000 edits plus the initial build:
938 successful commits and 63 intentional failure checks, with zero shared-output
mismatches. It performed 1,001 GC/recovery checks, removed 993 revision
directories, and retained at most nine directories (the recent eight plus an
older referenced owner). The workspace peaked at 12,835,921 bytes before GC.

Successful live edits had compiler p50/p95 of 34.521/65.188 ms; the independent
eager clean builds measured 754.639/1,176.996 ms. The live side reused 249 output
pages and dispatched 1,413,159 tokens over 937 successful edits; skipped execution
remained unmeasured. This is the same small 1–3-page synthetic fixture, on a shared
runner, and measures compiler calls rather than browser edit-to-visible latency.
The reduction removes durable checkpoint overhead while semantic compilation
continues cold. It does not establish the 10–30-page document latency target.

Selected edit categories from that same live run remain separate from each
other and from the single initial build, which took 610.957 ms:

| Successful live calls | Count | Compiler p50 | Compiler p95 |
| --- | ---: | ---: | ---: |
| Local text replacement | 126 | 34.162 ms | 56.814 ms |
| Preamble macro change | 63 | 34.880 ms | 59.880 ms |
| Explicit no-op compiler request | 62 | 28.360 ms | 54.315 ms |
| Page insertion/deletion | 124 | 47.961 ms | 115.971 ms |
| Comment/source-position change | 125 | 27.548 ms | 54.958 ms |

These categories still perform semantic execution. The no-op compiler calls
dispatched 86,118 tokens; the daemon's separate no-op-save regression verifies
that unchanged input hashes prevent making a compiler call at all. No local
versus global checkpoint-skipping claim follows from these timing differences.

The test took 836.58 seconds, excluding compilation, with exit status `0`.
The local evidence is
`.codex/live-preview-mutations/latexd-live-mutations-bpxkvX/verification-manifest.json`;
it identifies the report, executed test binary, source hash, and private log
`~/logs/latexd-published-live-thousand-1788846367983642106.log`.

### Native Integration Follow-up

The full 758-case eager smoke run initially passed 734 cases and failed 24.
Seventeen failures assumed a specific old checkpoint or preserved byte delta
despite changed scanner inputs; seven expected executed labels to be removed
from materialized source. The final text checks in those seven cases still
passed. The updated tests retain the output contract and add stronger checks:
stale scanner state must select the safe preamble boundary, recaptured tail
cursor/full VM state must agree with clean execution, and retained label
anchors must agree with actual shipout, aux pages, and original source offsets.

All 24 revised cases and six affected existing cases pass in focused follow-ups
(30 tests total). No product code changed during these test-contract corrections.
The full 758-case batch was not rerun after the test edits: its original
`734 passed / 24 failed`, exit `101`, remains in the evidence alongside the
successful focused runs. The latest separate library run passed 294 tests
with four explicitly ignored tests; the later changes are confined to test
code. The validation manifest at `.codex/review-implementation/validation.json`
records the source correspondence, logs, exit codes, and old/new test mapping.

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

### September 8 CC0 discovery run

The final local rerun produced internal PDFs for all six configured papers.
This is **six approximate builds**, not measured meaningful-render coverage.
The oracle was in discovery mode (`strict=false`); two page-count findings remain.
The original source/reference hashes are recorded in the local corpus lock.

| arXiv version | Internal / official pages | Normalized unique-token overlap | Smoke finding |
| --- | --- | --- | --- |
| 2602.14379v1 | 42 / 37 | 0.643 | Page-count tolerance exceeded |
| 2508.10038v1 | 15 / 14 | 0.934 | None under configured thresholds |
| 2404.05196v2 | 10 / 10 | 0.867 | None under configured thresholds |
| 2403.07956v1 | 11 / 10 | 0.938 | None under configured thresholds |
| 2102.03748v1 | 16 / 13 | 0.938 | Page-count tolerance exceeded |
| 2302.01837v1 | 16 / 17 | 0.842 | None under configured thresholds |

These measurements compare against the published PDFs, not reference builds in
a frozen TeX Live/package/font environment. Unique-token overlap does not verify
order, repeated words, math structure or citation correctness. The local raster
gate examines the first page with loose smoke thresholds; an empty findings list
is not full-paper fidelity or a contribution to a 90% compatibility claim.

Evidence: `.codex/corpus/cc0/reports-final/cc0-smoke-report.json`,
`.codex/corpus/cc0/corpus-lock.json`, and
`/home/seorii/logs/latexd-six-paper-final-1788841905201393667.log` (exit 0,
954.96-second oracle test). Earlier intermediate-run artifacts remain separate
in `reports-review`; they are not mixed into this final table.

## External Component Diff

The `latexd-tests` repository owns a set of small MIT-licensed fixtures that
isolate paragraphs, front matter, headings, formatting, lists, math, tables,
citations, references, links, columns, page breaks, line breaks, and minipages.
Each case compares `latexd` with `pdflatex` at the RenderEvent, Document IR,
PageDisplayList, extracted-text, page-count, and raster layers. This makes a
failure attributable to one syntax or layout contract before it is obscured by
a complete paper.

Every push and pull request calls the reusable component workflow at a pinned
`latexd-tests` commit and passes the exact `latexd` commit SHA. The resulting
artifact retains both PDFs, renderer logs, structured rendering output, and
color-coded page diffs because these fixtures are owned and redistributable.
The external licensed-paper workflow remains push-only and follows the stricter
artifact policy below.

The display-math fixture is a high-fidelity raster contract rather than a gross
smoke check: both its per-page minimum and mean ink IoU must be at least `0.90`.
The current Computer Modern TFM/Type1 path scores `0.963` while retaining page
parity and its extracted-text gates. This case pins article page geometry,
display skips, TeX math spacing, large-operator limits, fraction rules, script
metrics, embedded Type1 programs, and `/ToUnicode` mappings together. Other
component thresholds remain case-specific until their corresponding layout and
font contracts are implemented; raising one fixture does not imply full-paper
raster equivalence.

## External Licensed Corpus

The separately maintained
[`seo-rii/latexd-tests`](https://github.com/seo-rii/latexd-tests) repository
contains a license-aware manifest for well-known non-CC0 papers. It does not
vendor paper artifacts. Its fetcher pins the arXiv version and SHA-256, rejects
unsafe archives, downloads into an ephemeral/local cache, and prevents paper
sources, PDFs, extracted text, and raster derivatives from being uploaded as CI
artifacts.

The local oracle accepts that repository without copying its manifest into
`latexd`:

```bash
LATEXD_ARXIV_ORACLE_CORPUS=/tmp/latexd-tests-corpus \
LATEXD_ARXIV_ORACLE_MANIFEST=../latexd-tests/corpus/famous-arxiv-non-cc0.json \
LATEXD_ARXIV_ORACLE_REPORT_DIR=/tmp/latexd-tests-report \
  cargo test -p latexd --test arxiv_oracle -- --ignored --nocapture
```

`LATEXD_ARXIV_CC0_CORPUS` and the in-repository CC0 manifest remain the default
for backward compatibility. `LATEXD_ARXIV_ORACLE_REPORT_FILE` can override the
report filename for isolated case runs.

On trusted pushes to `main`, this repository calls the reusable workflow owned
by `latexd-tests` with the exact `github.sha`. This tokenless cross-repository
call keeps the corpus policy and harness in the external repository while
attaching the result to the latexd CI run. It is deliberately disabled for
pull requests: untrusted code must not receive remote-only paper artifacts.
Weekly full-corpus and manual runs execute from `latexd-tests` itself.

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

이 corpus는 **수정 후 clean build와의 동등성**을 먼저 확인하고,
무효화 범위와 HMR latency를 별도로 측정한다. 위의 native mutation harness는
현재 synthetic fixture의 동등성과 실제 build work trace를 기록한다.

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
