# Native Math Rendering And Browser Delivery Plan

This document is the execution plan for replacing the current string-oriented
math preview with a TeX-executed math model and for making that output visible
in the browser. It also separates browser delivery, font resolution, math
execution, math layout, and incremental compilation so progress in one layer is
not mistaken for correctness in another.

This plan supersedes the "common readable math subset" target in
[`real-rendering-plan.md`](./real-rendering-plan.md). Readable normalized text
remains useful for search, accessibility, diagnostics, and fallback, but it is
not a layout input and is not evidence of TeX-compatible math rendering.

VM-owned math depends on
[`vm-semantic-foundation-plan.md`](./vm-semantic-foundation-plan.md). In
particular, Eqtb/SaveStack, streaming tokenization, full macro parameter text,
and execution-generated semantic events must exist before `MathList` becomes an
authoritative artifact. This plan does not bypass that dependency with another
raw-source parser.

## Verified Baseline

As of 2026-07-26, the repository has four independent gaps.

| Layer | Current path | Observable failure |
| --- | --- | --- |
| Browser preview | `extracted_text` is split into 48-line synthetic pages | Math, tables, graphics, line breaking, and page geometry disappear |
| Math execution | source delimiters are scanned before VM execution and mostly become strings | expansion, catcodes, mathcodes, families, and local recovery are lost |
| Fonts and metrics | a narrow Computer Modern set and partial TFM data drive approximate runs | missing symbols and incorrect scripts, fractions, accents, and delimiters |
| Browser incrementality | each request instantiates a new WASI instance and VM | edits are full semantic re-executions despite module-byte reuse |

The first gap is directly visible in
[`browser-compiler.ts`](../web/apps/viewer/src/lib/browser-compiler.ts):
`toPages()` creates `wasi-page-*` identities from extracted text. Meanwhile,
[`latexd-wasi`](../crates/latexd-wasi/src/main.rs) already builds
`PageDisplayList` pages and writes `output.pdf`. The browser currently exposes
that PDF only as a download.

P0.1 closed this baseline gap on 2026-08-01. Browser-only builds now display
the generated `output.pdf` through an iframe using the same Blob URL as the
download action, and the synthetic extracted-text page path has been removed.
Explicit TeX errors fail the browser build while preserving the last successful
PDF. P0.2 is complete and the first direct display-list rendering slice is in
place; browser outline fidelity remains the active P0.3 work.

This means browser delivery is the first validation blocker, but it is not the
root cause of incorrect PDF math. The downloaded PDF and native/WASI display
lists remain the authoritative probes for the font, execution, and layout
layers.

## Compatibility Contract

"All LaTeX math" must be defined by explicit engine profiles rather than by an
unbounded list of package command names.

### Classic Math Profile

The first compatibility target is:

- TeX82/e-TeX math primitives;
- LaTeX2e math behavior implemented through those primitives;
- `amsmath` and `mathtools` core formula and alignment surfaces;
- classic TFM/VF math families and Type 1 output;
- pdfTeX-compatible formula geometry for supported fonts.

Pure macro packages should work through normal expansion once the required
primitives and box operations exist. A package is not marked supported merely
because a shim accepts or deletes its commands.

### Unicode Math Profile

The second compatibility target is:

- Unicode math character input;
- OpenType MATH constants, variants, assemblies, and math kerns;
- XeTeX/LuaTeX-style family and alphabet selection where the behavior is
  representable without engine-specific native extensions;
- `unicode-math` compatibility tracked by a dedicated capability matrix.

OpenType MATH is a font metric substrate, not a replacement for TeX math-list
execution or layout.

### Extension Profile

Engine-native extensions such as `\directlua`, shell escape, and drawing
systems such as TikZ are separate capabilities. They require an external tool,
plugin, prepared asset, or explicit diagnostic. They are not counted as native
math-layout support.

### Dual Output Policy

The project keeps two honest output modes during migration:

- **Fast Preview:** the internal VM, math model, layout, and display-list path;
- **Final TeX Output:** a pinned external pdfTeX/XeTeX/LuaTeX/Tectonic path.

The external path is the correctness oracle and package-compatibility fallback.
It must not be silently labeled as an incremental internal render.

## Architectural Invariants

```text
TeX tokens + macro/catcode/register state
  -> tex-vm math-mode execution
  -> MathFormula / MathList
  -> RenderEvent
  -> SemanticDocumentIr
  -> tex-math-layout
  -> MathBoxTree
  -> LayoutIr paragraph/page nodes
  -> PageDisplayList
  -> PDF / SVG / browser renderer

MathFormula
  -> Presentation MathML / speech / PDF ActualText
```

The following rules are non-negotiable:

- `tex-vm` decides what TeX execution produced.
- The VM may build `MathList`, because it is TeX execution state, but it must
  not build or mutate Document IR.
- `tex-layout` must not reparse raw math source on the authoritative path.
- Inline and display math use the same execution model and layout engine.
- `normalized_text` is never a math-layout input.
- Math layout uses TeX scaled integers; conversion to floating-point happens
  only at the renderer-facing boundary.
- Renderer backends draw positioned glyphs and rules. They do not choose math
  classes, spacing, styles, line breaks, delimiter sizes, or limits placement.
- Unsupported material degrades locally. One unknown command must not flatten
  or discard the rest of a formula.
- Native and WASI builds use the same model, metrics, and layout code.

## Model Ownership

Add a dependency-light `tex-math-model` crate once the first model patch lands.
It must not depend on `tex-vm`, `tex-layout`, `tex-fonts`, or renderer crates.

To avoid a dependency cycle with `tex-render-model`, math nodes carry compact
`MathSourceId` values. The containing render event owns the table that maps
those IDs to `SourceProvenance`.

```text
tex-math-model
  -> serde and scalar/hash helpers only

tex-render-model
  -> tex-math-model
  -> owns MathEvent provenance and diagnostics

tex-vm
  -> tex-math-model
  -> emits MathEvent through tex-render-model

tex-layout
  -> tex-math-model
  -> tex-fonts
  -> emits MathBoxTree, LayoutIr, and PageDisplayList
```

The initial execution model is:

```rust
pub struct MathFormula {
    pub schema_version: u32,
    pub mode: MathMode,
    pub list: MathList,
}

pub struct MathList {
    pub items: Vec<MathItem>,
}

pub enum MathItem {
    Noad(MathNoad),
    Fraction(MathFraction),
    Radical(MathRadical),
    Accent(MathAccent),
    OverUnder(MathOverUnder),
    Fence(MathFence),
    Choice(MathChoice),
    Style(MathStyle),
    Glue(MathGlue),
    Kern(Scaled),
    Penalty(i32),
    Rule(MathRule),
    Box(MathExecutionBox),
    Alignment(MathAlignment),
    Error(MathError),
}

pub struct MathNoad {
    pub class: MathClass,
    pub nucleus: MathField,
    pub superscript: Option<MathField>,
    pub subscript: Option<MathField>,
    pub limits: LimitsMode,
    pub source: MathSourceId,
}

pub enum MathClass {
    Ord,
    Op,
    Bin,
    Rel,
    Open,
    Close,
    Punct,
    Inner,
}

pub enum MathField {
    Empty,
    Character(MathCharacter),
    SubList(MathList),
    Box(MathExecutionBox),
}

pub struct MathCharacter {
    pub class: MathClass,
    pub family: MathFamilyId,
    pub code: MathCharacterCode,
    pub source: MathSourceId,
}

pub enum MathCharacterCode {
    LegacySlot(u8),
    Unicode(u32),
}

pub enum MathStyle {
    Display,
    DisplayCramped,
    Text,
    TextCramped,
    Script,
    ScriptCramped,
    ScriptScript,
    ScriptScriptCramped,
}
```

`MathExecutionBox` is an unpositioned TeX box produced by execution. It is not a
renderer box and must not contain `f32` page coordinates.

The renderer-facing result is a separate tree:

```rust
pub struct MathBox {
    pub width: Scaled,
    pub height: Scaled,
    pub depth: Scaled,
    pub axis: Scaled,
    pub italic_correction: Scaled,
    pub children: Vec<PositionedMathChild>,
    pub source: MathSourceId,
}
```

## Work Graph

Browser delivery is the first visual validation gate, but VM semantic
foundations are the correctness gate for math execution. The two proceed in
parallel where file ownership permits.

```text
P0 actual browser output -> native/WASI presentation parity
P1 classic font substrate -> native/WASI font parity

VM V3 Eqtb/SaveStack
  -> V4 streaming Mouth
  -> V5 macro/prefix model
  -> V6 execution SemanticSink
  -> P2 MathList

P1 font metrics + P2 MathList + VM V8 LayoutIr
  -> P3 TeX math layout
  -> P4 LaTeX/AMS math
  -> P5 Unicode math

P0 -> I1 worker isolation
VM V7 continuation snapshot -> I2 persistent session -> I3 stable-tail replay
```

## P0: Actual Browser Output

### P0.1 PDF Bootstrap (Complete)

Use the existing `output.pdf` as the first real browser preview through PDF.js
or an iframe fallback. Preserve the download action, but do not use
`extracted_text` as visible page content.

The TDD browser test now verifies:

- a successful WASI build displays the PDF result;
- the preview and download action reference the same generated PDF;
- the preview payload has a PDF header;
- fake `wasi-page-*` elements do not exist;
- a successful edit replaces the PDF Blob URL;
- a failed build preserves the last good PDF.

Completed exit criteria:

- downloaded PDF and browser preview consume the same artifact;
- visible page content no longer comes from `extracted_text`;
- explicit VM errors are distinguishable from recoverable diagnostics;
- preview failures can be classified separately from PDF-generation failures.

Compiler-owned page-count/geometry validation is part of P0.2. Formula, table,
and figure visual fixtures remain P0.3 acceptance gates, where the browser can
inspect rendered page artifacts instead of relying on the host PDF viewer.

### P0.2 Page Artifact Protocol (Complete)

WASI builds now write `/workspace/pages.json` and
`/workspace/build-meta.json`. The version 1 one-shot schema contains all pages
and marks each page as changed; the later session implementation will populate
the same changed, reused, and removed fields incrementally.

```rust
pub struct BrowserPagesArtifact {
    pub schema_version: u32,
    pub revision: u64,
    pub pages: Vec<PageDisplayList>,
    pub changed_page_ids: Vec<PageId>,
    pub removed_page_ids: Vec<PageId>,
    pub assets: Vec<BrowserAssetManifestEntry>,
}

pub struct BrowserBuildMetadata {
    pub schema_version: u32,
    pub revision: u64,
    pub compile_mode: BrowserCompileMode,
    pub event_count: u64,
    pub diagnostic_count: u64,
    pub pages: BrowserPageStats,
}
```

Page ID, order, size, content hash, source spans, and changed/removed state come
from the compiler. The browser must not synthesize them.

The browser rejects unsupported schemas, stale revisions, duplicate or empty
page identities, invalid geometry, inconsistent page counts, and invalid
changed/removed sets before replacing its last-good state. The E2E test verifies
revision and page-identity replacement after a successful edit and retention
after an explicit TeX error.

Image operations contribute their `asset_ref`, format, and content hash to an
explicit deduplicated manifest. The current paths resolve against the project
memfs; P0.3 must consume this manifest rather than perform native filesystem
discovery.

### P0.3 Display-List Browser Renderer (In Progress)

After the PDF bootstrap is stable, render `PageDisplayList` pages directly to
Canvas or SVG. Replace only changed page nodes, preserve scroll/zoom/source
selection, and retain PDF.js as a diagnostic and final-output comparison mode.

The browser renderer consumes positioned glyph IDs and the same bundled outline
fonts used by PDF output. CSS text is allowed only as a diagnosed fallback;
otherwise browser shaping and font substitution would invalidate native/WASI
geometry parity.

The first renderer slice landed on 2026-08-01:

- each compiler page is an SVG view box keyed by its stable `page_id`;
- `Save`, `Restore`, and `ClipRect` are lowered to per-operation clip state;
- text runs, rules, image boxes, link annotations, and named destinations have
  direct SVG representations;
- unsupported image payloads and unbalanced graphics state remain visible as
  local fallbacks or diagnostics instead of deleting the page;
- Fast preview and PDF output are explicit modes backed by the same build;
- every CSS-shaped text run is counted and marked as a fallback.
- PNG, JPEG, and SVG manifest entries resolve from browser memfs to
  lifecycle-safe Blob URLs; missing, unsafe, PDF, EPS, and unknown assets remain
  explicit diagnostics or page-local fallbacks.
- file-backed text, image, and blocked-link operations expose keyboard-accessible
  source targets; hover reports the compiler path/line/column and activation
  selects the exact textarea range;
- compiler UTF-8 byte spans are converted only at valid code-point boundaries to
  JavaScript UTF-16 indices, and selection resolves against the last successful
  build's source snapshot so a failed edit cannot silently retarget old pages.
- Fast Preview exposes 50–200% zoom in 10% steps without changing display-list
  coordinates or PDF output;
- the keyed `page_id` loop preserves unchanged page DOM nodes, and the
  multi-page E2E gate verifies that replacing a middle page retains the visible
  tail page's scroll offset.

The renderer-neutral positioned-glyph contract is also in place. A resolved
text run carries one `ResolvedFontRef` containing the stable face ID,
PostScript name, glyph-ID interpretation, and a BLAKE3 identity derived from
the exact metric and outline bytes. Its glyph array contains glyph IDs,
advances, and run-relative positions, while UTF-8 clusters preserve the
logical-text mapping. Native and WASI layout fill this data from `tex-fonts`;
the resolved font content hash plus glyph geometry participates in the page
content hash.

The first hermetic font slice is complete. `tex-fonts` embeds unmodified
Computer Modern TFM and Type 1 files for `cmr10/7/5`, `cmmi10/7/5`,
`cmsy10/7/5`, and `cmex10`. The default resolver checks this bundle first in
both native and WASI builds, then permits Kpathsea discovery only as a native
fallback for faces outside the bundle. The checked-in manifest records source
archive and per-file SHA-256 values; the AMS Type 1 files retain OFL-1.1 and
the TFM files retain the Knuth License instead of being relicensed as MIT.
Hermetic tests verify every bundled payload against the manifest, and the WASI
browser E2E gate verifies that a `cmr10` content hash and positioned glyphs
reach the page artifact.

This is not yet browser outline parity. The browser does not receive the Type 1
outline program and therefore still marks and draws these runs as an explicit
CSS-shaped fallback. Bundling fixes deterministic face selection and metrics;
it does not make host CSS glyph shapes equivalent to Computer Modern.

Remaining P0.3 work:

- expose bundled glyph outlines as a browser artifact and draw positioned SVG
  paths instead of CSS-shaped text;
- make native/WASI display-list parity a gate before removing the diagnosed CSS
  fallback;
- add formula, table, image, link, and multi-page visual regression fixtures.

## P1: Font And Metric Substrate

The first common resolver slice is implemented as:

```rust
pub trait FontResolver {
    fn resolve_tfm(&self, stem: &str) -> Option<FontData>;
    fn resolve_type1(&self, stem: &str) -> Option<FontData>;
}
```

Virtual-font and encoding lookups remain planned extensions; they are not
represented by placeholder methods in the current API.

Implement:

- `BundledFontResolver` for the deterministic native/WASI baseline;
- `KpathseaFontResolver` as a validated native-only fallback;
- a versioned, hashed, license-audited font manifest;
- bundle-first resolution in both environments.

`PageDisplayList` already has the consumer-side seam for this work:
`PositionedTextRun::resolved_font`, positioned glyphs, and UTF-8 text clusters.
The resolved binary/content hash is now part of `ResolvedFontRef` and the page
content hash; a face name alone is not sufficient to prove renderer parity.

The first classic bundle includes `cmr10/7/5`, `cmmi10/7/5`,
`cmsy10/7/5`, and `cmex10` TFM and Type 1 data. `cmsy` is available through
the resolver but is deliberately not selected by the current Unicode-string
symbol fallback: structured MathList family/slot mapping must become
authoritative first. Encoding and virtual-font support remain follow-up work.

The TFM/VF layer must preserve:

- width, height, depth, and italic correction;
- ligature/kern programs;
- next-larger links and extensible recipes;
- font dimensions and math parameters;
- virtual-font packet mappings;
- text, script, and scriptscript family selection.

Required math parameters include numerator/denominator shifts, superscript and
subscript shifts/drops, delimiter sizes, axis height, default rule thickness,
and large-operator spacing.

Exit criteria:

- the classic math atlas has no unexpected missing glyphs;
- native and WASI resolve the same faces and metrics from the test bundle;
- missing/fallback faces produce structured diagnostics;
- layout code no longer invents fixed percentages when a TeX metric exists.

## P2: VM-Owned Math Execution

Replace source pre-scanning with a demand-driven math-mode builder inside
`tex-vm`.

Prerequisites:

- Eqtb and SaveStack own catcode, mathcode, delcode, register, and font state;
- the production VM uses a streaming Mouth;
- macros preserve full parameter text and expansion provenance;
- VM nest state distinguishes horizontal, vertical, and math modes;
- inline/display boundaries are emitted through the VM-owned SemanticSink;
- whole-source scanner events are recovery/debug data, not production input.

The first execution slice covers:

- `$`, `$$`, `\(`, `\)`, `\[`, and `\]`;
- grouping, expansion, conditionals, and active characters in math mode;
- `\mathcode`, `\delcode`, `\fam`, math-family assignments, `\mathchar`,
  `\mathchardef`, `\mathaccent`, `\radical`, and `\delimiter`;
- `^`, `_`, primes, `\limits`, `\nolimits`, and `\displaylimits`;
- `\mathchoice`, styles, `\nonscript`, `\mathsurround`, `\mkern`, `\mskip`,
  thin/medium/thick math glue, and penalties;
- `\over`, `\atop`, `\above`, and their delimiter forms;
- radicals, accents, fences, `\vcenter`, boxes, and rules;
- `\delimiterfactor`, `\delimitershortfall`, `\nulldelimiterspace`,
  `\scriptspace`, and math-font state needed by layout;
- local error nodes and balanced recovery.

Tests are written in this order:

1. macro-expanded and literal formulas produce equivalent `MathList` goldens;
2. catcode/mathcode/family assignments change the resulting nodes;
3. inline and display use the same nodes with only mode/style differences;
4. one unsupported command preserves the valid prefix and suffix;
5. source provenance identifies invocation, argument, and generated nodes;
6. snapshot/replay at formula boundaries is byte-stable.

The first checkpoint policy only permits new checkpoints outside an open math
builder. Serializing an in-progress builder is a later optimization, not a
first-slice correctness requirement.

Exit criteria:

- migrated math never depends on `parse_display_math_structure()`;
- `normalized_text` is used only as derived text;
- unsupported commands create one local diagnostic and one local error node;
- inline and display formulas share one model and one execution path.

## P3: TeX Math List-To-Box Layout

Implement the classic `mlist_to_hlist` behavior as a renderer-neutral,
fixed-point transformation.

Prerequisite: the VM foundation plan's renderer-neutral `LayoutIr` boundary is
available so math boxes enter paragraph/page layout without bypassing the
common hlist/vlist model.

The first pass:

- recursively lays out nuclei and sublists;
- resolves style transitions and cramped styles;
- reclassifies invalid binary operators as ordinary atoms;
- lays out scripts, fractions, operators, radicals, accents, and fences;
- computes maximum height/depth and math-axis relationships.

The second pass:

- inserts the full class-to-class math spacing table;
- suppresses style-dependent glue where TeX does;
- inserts penalties where required;
- emits positioned glyphs, rules, kerns, and nested boxes.

Construct order:

1. atom classes, binary demotion, and spacing;
2. eight styles and script placement;
3. fractions and ruleless fractions;
4. operators and side/limit scripts;
5. radicals and accents;
6. fixed and extensible delimiters;
7. choices, explicit glue/kern, boxes, and local errors.

Exit criteria:

- every construct has `MathList`, box geometry, and draw-op goldens;
- classic atlas formula crops pass registered IoU >= 0.90;
- baseline, axis, rule thickness, and relative child positions also pass
  numeric tolerances;
- inline and display output differ only where TeX style rules require it.

## P4: LaTeX And AMS Formula Structures

Build `array`, `matrix`, `cases`, `aligned`, `gathered`, `split`, `align`, and
equation tags on a shared alignment/box model. Do not flatten rows into
semicolon-delimited text.

Track package compatibility by behavior:

- parsed and laid out;
- macro-compatible through implemented primitives;
- external/final-output only;
- locally degraded with diagnostic;
- unsupported.

`amsmath`, `mathtools`, `amssymb`, `bm`, and common scientific packages receive
separate capability rows. Loading a package without executing its visible
behavior does not advance the row.

## P5: Unicode And OpenType MATH

Add a renderer-neutral OpenType math metrics adapter with:

- MATH constants;
- italic correction and top-accent attachment;
- four-corner math kerns;
- glyph variants and assemblies;
- script-style alternates;
- logical text, glyph ID, and cluster mapping.

Text embedded in math uses the common shaping adapter. Math layout remains in
`tex-math-layout`; Skia or another backend must not become the canonical line
breaker or formula layout engine.

## Incremental Lane

Worker isolation starts after P0 and can proceed in parallel. A persistent
semantic session does not begin until continuation snapshot equivalence from
the VM foundation plan is green.

### I1 Worker Isolation

Move browser compilation to a Web Worker. Add revision ordering, cancellation,
request coalescing, and last-good-preview retention. This removes UI blocking
but is not yet semantic incremental compilation.

### I2 Persistent Compiler Session

Replace repeated WASI `_start` execution with a library-style session ABI that
instantiates once and owns:

```rust
pub struct CompilerSession {
    pub project: ProjectSnapshot,
    pub vm: TeXVm,
    pub checkpoints: CheckpointStore,
    pub dependencies: DependencyGraph,
    pub pages: PageStore,
    pub fonts: FontCache,
    pub aux: AuxState,
}
```

Start with safe checkpoints at preamble completion, successful shipout,
top-level input boundaries, and explicitly verified empty page-builder
boundaries.

Prerequisite: `FormatSnapshot`, `ContinuationCheckpoint`, and transactional
semantic/trace/diagnostic sinks pass full-build versus restore/replay
equivalence.

### I3 Replay And Stable-Tail Reuse

Reuse the old tail only when all of these match:

- VM execution-state hash;
- next shipped page/display-list hash;
- dependency read set;
- semantic aux state;
- renderer and font-metric version keys.

Page hash equality alone is insufficient because counters, marks, floats,
labels, and output-routine state can affect later pages.

Exit criteria:

- a body edit reuses an unaffected prefix;
- replay stops at a state-stable and page-stable boundary;
- a preamble or mathcode change conservatively invalidates dependent pages;
- build metadata reports replay tokens, checkpoint kind, changed/reused pages,
  math fallbacks, and font fallbacks.

## Following Document-Fidelity Milestone

Completing math does not by itself make the internal renderer a complete LaTeX
typesetter. The next milestone reuses the same box, font, and page artifacts
instead of creating parallel approximations.

Required follow-on work:

- general `HBox`, `VBox`, rule, glue, kern, penalty, insertion, mark, and
  whatsit nodes;
- a shared alignment engine for math arrays and text tables;
- `tabular`, `\halign`, spans, rules, width distribution, and long-table page
  splitting;
- graphics sizing, clipping, trim/viewport, rotation, and browser-safe asset
  decoding/conversion;
- colors, links, destinations, and annotations through `PageDisplayList`;
- footnote insertion, floats, columns, page penalties, and output-routine
  compatibility;
- package capability reports backed by behavior fixtures rather than load-only
  or no-op shim tests.

The WASI profile must diagnose formats that require unavailable external tools.
Native-only conversion success must not be reported as browser parity.

## Test Strategy

### Math Atlas

Create an owned source atlas with one small formula per behavior:

- atom classes and unary/binary demotion;
- all eight styles;
- nested scripts and primes;
- fractions, radicals, operators, and limits;
- fixed and extensible delimiters;
- accents and over/under constructions;
- matrices, cases, alignments, and tags;
- classic and Unicode math alphabets;
- macro expansion, active characters, mathcodes, and local failures.

For every atlas case, retain:

- source and expected diagnostics;
- `MathList` semantic golden;
- `MathBoxTree` geometry golden;
- display-list operation golden;
- native/WASI parity result;
- pinned reference-engine PDF and formula crop.

### Differential Rendering

Render formula crops at 144-300 DPI against pinned pdfTeX/Tectonic and later
XeTeX/LuaTeX profiles. Apply a bounded translation registration before
measuring IoU. Also measure baseline, math axis, bounding box, distance-field
error, glyph coverage, and extracted-text order.

Whole-page IoU remains useful for pagination regressions but is not the primary
math metric.

### CI Layers

Default CI:

- model serialization and math-list goldens;
- box geometry and display-list goldens;
- compact font bundle and native/WASI parity;
- browser PDF/page-artifact tests;
- small registered raster atlas.

Push CI:

- broader component atlas;
- selected classic package and alignment fixtures;
- external-reference differential report artifact.

Nightly/manual CI:

- generated grammar/metamorphic formulas;
- full licensed paper corpus;
- Unicode/OpenType profile;
- incremental multi-revision performance and cache sweeps.

## Observability Contract

`build-meta.json` must identify the layer that degraded:

```json
{
  "compile_mode": "incremental",
  "checkpoint": { "restored": true, "kind": "shipout" },
  "replay": { "tokens": 4192, "files": 2 },
  "pages": { "total": 12, "changed": 1, "reused": 11 },
  "math": {
    "expressions": 39,
    "structured": 36,
    "fallback": 3,
    "unsupported_commands": 2,
    "missing_glyphs": 1
  },
  "fonts": { "resolved": 8, "fallbacks": 0 }
}
```

Required diagnostic codes begin with:

- `MATH_UNSUPPORTED_COMMAND`;
- `MATH_MISSING_GLYPH`;
- `MATH_LAYOUT_LIMIT`;
- `FONT_RESOLUTION_FALLBACK`;
- `PACKAGE_SHIM_NOOP`;
- `INCREMENTAL_CHECKPOINT_MISS`;
- `BROWSER_PAGE_ARTIFACT_MISMATCH`.

## Resource Limits And Recovery

The VM and layout engine enforce explicit budgets for expansion steps, math-list
nodes, nesting depth, alignment rows/cells, delimiter assembly pieces, and
diagnostic payload size.

Recovery occurs at balanced group, field, row, or formula boundaries. It must
not silently delete visible input or convert the entire expression to one text
run.

## Commit-Sized Delivery Order

1. Browser PDF bootstrap and compiler-owned page artifacts can proceed directly
   in the browser/WASI lane.
2. Extend the completed hermetic `FontResolver`/classic-manifest slice with
   full TFM math metrics, encoding/VF support, and browser outline artifacts.
3. Complete the direct VM foundation batches through execution-owned
   `SemanticSink`.
4. Add `tex-math-model` serialization and source-reference goldens.
5. Add the VM math builder for atoms, groups, styles, and scripts.
6. Add fractions, operators, radicals, accents, delimiters, and local recovery.
7. Complete TFM metrics and fixed-point math list-to-box differential gates.
8. Add the shared alignment engine for matrices, cases, AMS rows, and tags.
9. Complete continuation snapshots before persistent `CompilerSession`.
10. Add stable-tail replay and build metadata.
11. Add OpenType MATH and the Unicode profile.
12. Retire raw-source layout parsing and Unicode symbol special cases.

Each commit must leave legacy behavior available only where the next boundary
has not migrated. A migrated capability requires a model golden, layout golden,
and visible-output test before its legacy branch is removed.

## Explicit Non-Solutions

The following do not count as progress toward native TeX math:

- adding more command-to-Unicode substitutions as the canonical path;
- using `normalized_text` to position formulas;
- separate inline and display parsers;
- flattening matrices or alignments into prose;
- treating package no-op shims as support;
- resolving WASI fonts through runtime `kpsewhich`;
- letting PDF, SVG, Canvas, or Skia choose math metrics or layout;
- starting authoritative MathList generation before VM execution owns semantic
  events;
- calling repeated fresh-VM builds incremental;
- using only whole-page IoU to claim formula fidelity.

## Reference Implementations

- Tectonic/XeTeX math code is an executable geometry oracle and a reference for
  TeX math-list behavior:
  <https://github.com/tectonic-typesetting/tectonic/blob/master/crates/engine_xetex/xetex/xetex-math.c>
- The OpenType MATH table defines the Unicode-math metric substrate:
  <https://learn.microsoft.com/en-us/typography/opentype/otspec190/math>
- Typst provides a useful Rust/OpenType MATH implementation reference:
  <https://github.com/typst/typst/tree/main/crates/typst-layout/src/math>
- KaTeX is useful for atom, spacing, delimiter, and test organization, but is
  not the compatibility definition:
  <https://github.com/KaTeX/KaTeX/tree/main/src>
- MathJax and MathML are references for the accessibility sidecar, not the
  canonical TeX layout model:
  <https://docs.mathjax.org/en/v4.0/advanced/model.html>
- PDF.js is the browser bootstrap renderer:
  <https://github.com/mozilla/pdf.js>

Reference source is not copied without a per-file license audit. Behavioral
oracle use and clean-room reimplementation are the defaults.
