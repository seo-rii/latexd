# Real Rendering Plan

This document defines the path from the current internal `latexd` PDF scaffold to a
real preview renderer. It focuses on practical arXiv paper rendering, not on
implementing every TeX primitive before useful output is possible.

## Why This Exists

The current internal compiler can now build representative CC0 arXiv papers with
zero VM diagnostics, but the output is still a text scaffold:

- the VM expands source into a mostly linear string;
- `tex-layout` wraps that string by a fixed character budget;
- `tex-pdf` draws each line with one simple Helvetica text operation;
- class/package shims often consume formatting commands instead of producing
  structured output;
- figures are represented as `[image]`;
- math commands are mostly ASCII text stubs;
- citations can leak raw citation keys.

That is enough for compile-path smoke testing and incremental infrastructure, but
it is not real rendering. The arXiv oracle overlap ratios reflect that gap: some
papers have high text recall, while papers with heavy title pages, figures,
class formatting, citations, and math vocabulary stay much lower.

## Current Baseline

Strict CC0 oracle run after the Phase 2 page-model pass:

| arXiv id | Internal build | Diagnostics | Pages | Raster gross | Internal/oracle token count | Unique-token overlap |
| --- | --- | ---: | ---: | --- | ---: | ---: |
| `2602.14379` | ok | 0 | `36 / 37` | pass | 0.862 | 0.600 |
| `2508.10038` | ok | 0 | `16 / 14` | pass | 0.894 | 0.747 |
| `2404.05196` | ok | 0 | `11 / 10` | pass | 0.738 | 0.579 |
| `2403.07956` | ok | 0 | `8 / 10` | pass | 0.583 | 0.538 |
| `2102.03748` | ok | 0 | `13 / 13` | pass | 0.833 | 0.836 |
| `2302.01837` | ok | 0 | `17 / 17` | pass | 0.708 | 0.592 |

The important reading is:

- diagnostics are no longer the primary blocker for these papers;
- every CC0 smoke case is now inside the configured page-count tolerance and
  first-page raster gross checks pass;
- token-count recall can be decent, so large parts of body text already survive;
- unique-token overlap is pulled down by missing front matter, figure text,
  citation formatting, Unicode/math normalization, and raw macro/citation output;
- real rendering must be measured with structure and raster checks, not only
  unique token overlap.

## Product Goal

The product goal is a fast internal preview that is visually and semantically
close enough for edit-preview workflows.

The first target is not publication-grade PDF output. The first target is:

- correct high-level page structure;
- readable title, authors, abstract, sections, paragraphs, lists, tables,
  figures, bibliography, and common math;
- page count close to external LaTeX for normal papers;
- stable page identities and source spans for incremental preview;
- useful raster similarity checks on representative papers.

## Non-Goals

These are explicitly not required for the first real-rendering milestone:

- byte-for-byte PDF equality;
- pixel-perfect TeX paragraph breaking;
- full TeX82 page builder compatibility;
- complete LaTeX kernel execution;
- full font selection fidelity;
- every package's visual behavior;
- TikZ/PGF native drawing;
- exact bibliography style reproduction before citation text is no longer wrong.

These can be future compatibility lanes, but treating them as prerequisites would
block useful progress.

## Success Metrics

Use layered metrics so one weak metric does not hide real progress.

### Required Smoke Metrics

- internal build succeeds;
- VM diagnostics are zero or classified as allowed non-rendering warnings;
- page count is within a configured tolerance;
- extracted internal token count is at least `0.85` of oracle token count for
  normal text-heavy papers;
- unique-token overlap is at least `0.80` after normalization for the CC0 smoke
  corpus;
- no page is unexpectedly blank;
- no page has obvious text overflow outside media box.

### Raster Metrics

Raster comparison should start as a gross-regression detector:

- render first `N` pages of official and internal PDFs with `pdftoppm` or
  Ghostscript;
- compare page dimensions;
- compare non-white bounding boxes;
- compare downsampled luminance images;
- record diff images, but do not fail on small antialiasing differences;
- initially gate only catastrophic differences: blank pages, missing major text
  blocks, page-size mismatch, or fully wrong page count.

### Semantic Metrics

Track semantic surfaces separately from visual output:

- title text;
- author names;
- abstract text;
- section headings;
- labels;
- citations;
- bibliography entries;
- figure/table captions;
- page-to-source span coverage.

## Architecture Direction

The main architectural change is to stop treating VM output as one string.
The boundary decision is now accepted: `tex-vm` emits typed `RenderEvent`s, and
a separate builder derives `Document IR`. Read
[`real-rendering-accepted-structure.md`](real-rendering-accepted-structure.md)
before starting broad implementation work.

### Current Pipeline

```text
TeX source -> lexer -> VM expansion -> String -> fixed-width layout -> simple PDF
```

### Target Pipeline

```text
TeX source
  -> lexer
  -> VM + LaTeX semantic hooks
  -> RenderEvent stream
  -> Document IR builder
  -> Document IR
  -> layout tree / boxes
  -> page builder
  -> PageDisplayList
  -> renderer backend
       -> PDF/SVG/raster artifacts
```

The VM should still execute enough macros to discover document content, but
rendering commands should emit typed `RenderEvent`s instead of dumping raw tokens
or mutating `Document IR` directly. `Document IR` is a stable semantic artifact
derived from events. `PageDisplayList` is the stable renderer input.

The accepted boundary decision is documented in
[`real-rendering-design-question.md`](real-rendering-design-question.md):

```text
TeX VM execution
  -> RenderEvent stream
  -> Document IR builder
  -> Document IR
  -> layout/page builder
  -> PageDisplayList
  -> renderer backend
```

The VM decides what TeX execution produced. The IR builder decides what document
structure that output represents. The layout engine decides where it goes. The
renderer only draws already-positioned page operations.

The accepted first-batch structure, crate split, event schema, provenance model,
fallback contract, CI gates, and remaining deferred decisions are collected in
[`real-rendering-accepted-structure.md`](real-rendering-accepted-structure.md).

## Document IR

Introduce a stable internal document representation before building a full page
builder.

Suggested first IR:

```text
Document
  Block[]

Block
  TitleBlock { title, authors, affiliations, date, notes }
  Abstract { inline_content }
  Heading { level, number, inline_content, source_span }
  Paragraph { inline_content, source_span }
  DisplayMath { math_content, source_span }
  Figure { asset_ref, width_hint, height_hint, caption, source_span }
  Table { rows, caption, source_span }
  List { kind, items, source_span }
  Bibliography { items, source_span }
  RawFallback { text, source_span, reason }

Inline
  Text
  Emphasis
  Strong
  Monospace
  Link
  Citation
  InlineMath
  Symbol
  Space
```

Rules:

- every IR node should carry source span where practical;
- unknown constructs should become `RawFallback`, not disappear silently;
- IR should preserve enough structure for layout and sync, but not attempt to be
  TeX's exact box list in the first milestone;
- class/package shims should emit high-level `RenderEvent`s where they
  intentionally abstract over full LaTeX behavior;
- shims must not mutate `Document IR` directly.

## Render Events And Display Lists

The first rendering migration should introduce two additional stable artifacts:

- `RenderEvent`: the boundary between VM execution and semantic recovery;
- `PageDisplayList`: the boundary between layout and renderer backends.

First event families:

- `Text`, `Space`, and `ParagraphBreak`;
- `SetDocumentMetadata` and `FlushTitleBlock`;
- `BeginBlock`, `EndBlock`, and `Heading`;
- `InlineCitation` and `BibliographyItem`;
- `GraphicRef` and `Caption`;
- `InlineMath` and `DisplayMath`;
- `RawFallback` and `Diagnostic`.

First `PageDisplayList` operations:

- positioned text runs;
- rectangles/rules;
- images;
- links and named destinations;
- clipping/save-restore;
- page/source metadata.

Text shaping should eventually happen before final display-list emission through
a renderer-neutral shaping adapter. Skia can be one adapter/backend, but it
should remain optional behind a feature flag until the display-list, font, and
text contracts are stable.

VM checkpoints stay renderer-neutral. Event segments, IR, layout,
display-lists, shaped runs, decoded assets, and rendered tiles are all derived
caches with independent invalidation keys.

## Workstreams

### A. Oracle And Diagnostics

Purpose: make progress measurable before touching layout deeply.

Tasks:

- extend `arxiv_oracle` report with page count;
- keep official/internal extracted text paths or persist selected text snippets;
- add raster smoke fields: page dimensions, non-white bbox, diff artifact paths;
- normalize text before unique overlap: Unicode Greek to names or names to
  Unicode, common ligatures, soft hyphenation, citation-number noise;
- split text metrics into body text, front matter, captions, references where
  the IR can identify them;
- add case-level budgets for known hard classes: text-heavy, figure-heavy,
  math-heavy, bibliography-heavy.

Done when:

- the report explains why a case failed or degraded;
- a low ratio can be attributed to front matter, citations, figures, math, or
  layout instead of requiring manual inspection every time.

### B. Front Matter And Top-Level Structure

Purpose: recover a large amount of visible paper text currently consumed by
stubs.

Tasks:

- store `\title`, `\author`, `\date`, `\thanks`, `\affil`, `\institute`,
  `\email`, `\keywords`;
- implement `\maketitle` as a structured `TitleBlock`;
- support common class variants used by `article`, `llncs`, `IEEEtran`,
  `revtex4-2`, and `wacv`;
- preserve abstract content as an `Abstract` block;
- emit headings as heading nodes, not plain text only;
- preserve author notes without leaking footnote macro syntax;
- emit these surfaces first as `RenderEvent`s and let the IR builder construct
  `TitleBlock`, `Abstract`, and `Heading` nodes.

Done when:

- official title and author tokens appear in internal output for the CC0 corpus;
- front matter no longer dominates missing-token samples;
- source spans still point back to the originating preamble/body commands.

### C. Citations And Bibliography

Purpose: stop raw citation keys from polluting rendered text.

Tasks:

- represent citation commands as `InlineCitation` events with keys and style
  hints;
- render unknown citations as `[?]` or a stable compact placeholder, never raw
  keys;
- parse available `.bbl` entries into semantic records and bibliography events;
- map citation keys to numeric labels when `.bbl` is available;
- render bibliography items as paragraphs/list items;
- keep citation semantic aux unchanged for incremental correctness.

Done when:

- extra-token samples no longer contain large citation-key families;
- references contribute visible bibliography text when `.bbl` exists;
- missing bibliography text is reported separately from body text overlap.

### D. Graphics And Figures

Purpose: replace `[image]` placeholders with real page content.

Tasks:

- resolve `\includegraphics` to an asset with format and dimensions;
- support PDF, PNG, JPG, and EPS-through-existing-conversion where available;
- add `Figure` IR with asset ref and caption;
- implement PDF image embedding or page-level raster insertion in `tex-pdf`;
- preserve caption text even if image rendering fails;
- expose missing/unsupported image diagnostics as non-fatal render warnings.

Done when:

- figure-heavy papers no longer show major blank regions;
- figure captions appear near image placeholders/assets;
- raster smoke can detect missing images.

### E. Paragraph Layout And Page Builder

Purpose: move from fixed-width text wrapping to page-aware layout.

Tasks:

- introduce block layout with margins, font size, line height, and spacing;
- implement paragraph line breaking using font metrics rather than character
  count;
- support headings, abstract indentation, list indentation, and bibliography
  indentation;
- implement page breaks based on accumulated block height;
- preserve stable page ids from content hashes and source spans;
- keep existing incremental checkpoint/page metadata contracts intact.

Done when:

- page count is close on text-heavy papers;
- no gross overflow on normal paragraphs;
- same edit still produces stable unchanged page ids where content did not move.

### F. Tables

Purpose: handle the common `tabular`/`booktabs`/`longtable` surface enough for
papers.

Tasks:

- parse tabular rows and cells into `Table` IR;
- support `&`, `\\`, `\hline`, `\cline`, `\toprule`, `\midrule`,
  `\bottomrule`;
- support basic `l`, `c`, `r`, and paragraph columns;
- handle `\multicolumn` and `\multirow` as approximations;
- render table caption and label;
- fall back to monospace text table when structure is too complex.

Done when:

- tables are readable;
- table content is no longer flattened into broken inline text;
- table captions and labels remain discoverable.

### G. Math Rendering

Purpose: make common paper math readable without requiring full TeX math layout
up front.

Tasks:

- represent inline and display math distinctly;
- map Greek symbols and common operators to renderable glyphs;
- support superscript/subscript runs;
- support fractions, roots, hats/bars/vectors, delimiters, sums/products,
  integrals, and matrices as staged subsets;
- keep raw math source as fallback for unsupported constructs;
- normalize math text in oracle metrics so ASCII stubs and Unicode glyphs do not
  create misleading failures.

Done when:

- math-heavy papers no longer lose large vocabulary sets;
- formulas are readable enough for preview;
- unsupported math degrades locally instead of corrupting the whole paragraph.

### H. Class And Package Semantic Shims

Purpose: avoid executing full formatting packages while preserving their visible
semantic output.

Tasks:

- keep class/package shims, but make them semantic rather than purely
  diagnostic-suppression stubs;
- for `llncs`, emit title/institute/keywords/abstract semantics;
- for `IEEEtran`, emit IEEE author blocks and common biography/thanks surfaces;
- for `revtex4-2`, emit affiliations, PACS/keywords where present, and title
  structure;
- for `wacv`, emit camera-ready title block, abstract, captions, and section
  formatting;
- document every shim as "semantic approximation" with supported visible
  surfaces.

Done when:

- local class files no longer need to be fully interpreted for preview-quality
  output;
- each shim has corpus coverage and visible output expectations.

## Phase Plan

### Phase 0: Measurement Upgrade

Scope:

- add page count and raster smoke to the oracle;
- persist enough report artifacts to inspect failures;
- add normalized text metrics.

Exit criteria:

- every CC0 smoke case reports page count, raw ratio, normalized ratio, and
  raster gross status;
- low ratio causes are classified.

### Phase 1: Semantic Text Recovery

Scope:

- front matter;
- abstract;
- headings;
- citations not leaking raw keys;
- `.bbl` bibliography text.

Exit criteria:

- CC0 smoke normalized unique overlap reaches `0.80+` for at least five of six
  cases;
- no extra-token sample is dominated by citation keys;
- title/author/abstract appear in internal extracted text.

### Phase 2: Document IR And Block Layout

Scope:

- introduce `Document IR`;
- convert current VM output path to emit paragraphs/headings/title blocks;
- implement block layout and page builder with font metrics;
- keep source span and page metadata compatibility.

Exit criteria:

- page count is within a small configured tolerance on text-heavy CC0 cases;
- first-page raster smoke no longer flags missing major text blocks;
- unchanged page identity tests still pass.

### Phase 3: Figures And Tables

Scope:

- real image resolution and embedding/raster insertion;
- figure captions;
- table IR and basic rendering.

Exit criteria:

- figure-heavy cases no longer show large blank image regions;
- tables are readable in raster output;
- extracted caption/table text is present.

### Phase 4: Math Subset

Scope:

- inline/display math IR;
- common symbols and operators;
- superscript/subscript/fraction/root/accent subset;
- math fallback policy.

Exit criteria:

- math-heavy cases improve normalized text and raster smoke;
- unsupported math is visibly bounded and reported.

### Phase 5: Incremental Real Rendering

Scope:

- connect IR/page builder to checkpoint replay;
- preserve stable block/page ids;
- keep replay invalidation conservative but useful;
- measure warm edit latency against current scaffold path.

Exit criteria:

- body edit reuses unaffected pages;
- source sync still lands on the expected page/block;
- preview latency remains acceptable for arXiv-scale papers.

## Testing Plan

Add tests in layers.

Micro tests:

- `\maketitle` produces `TitleBlock`;
- citation commands produce `Citation`, not raw key text;
- `.bbl` parser emits bibliography items;
- includegraphics resolves asset refs;
- tabular parser handles rows/cells;
- math parser handles core subset.

Compat fixtures:

- article title/abstract/sections;
- llncs title/institute/keywords;
- IEEE author blocks;
- revtex affiliation/keywords;
- figure with PDF image;
- booktabs table;
- bibliography with `.bbl`;
- inline and display math subset.

Oracle tests:

- current CC0 smoke corpus;
- strict text metric mode;
- raster gross-diff mode;
- per-case report artifacts.

Regression rule:

- a shim may suppress unsupported formatting complexity, but it must not silently
  discard visible semantic content that the class/package normally prints.

## Risk Register

### R1: Full TeX Compatibility Sink

Risk: trying to execute stock LaTeX/classes faithfully before producing useful
preview output.

Mitigation: prefer semantic shims and IR hooks for visible content; keep full
compatibility as a separate long-term lane.

### R2: IR Diverges From Incremental Model

Risk: block/page layout breaks existing source span, checkpoint, and page reuse
contracts.

Mitigation: every IR node carries source span; page metadata tests must be
updated before replacing the string layout path.

### R3: Metrics Reward The Wrong Thing

Risk: unique-token overlap punishes harmless normalization differences while
missing visual failures.

Mitigation: use raw text, normalized text, semantic surfaces, page count, and
raster smoke together.

### R4: Image Rendering Scope Creep

Risk: embedding every image/PDF/vector format becomes a renderer project by
itself.

Mitigation: start with PDF/PNG/JPG assets and a placeholder-with-caption fallback;
classify unsupported assets.

### R5: Math Rendering Scope Creep

Risk: full math layout stalls the project.

Mitigation: render a useful subset first and keep raw math fallback visible.

## Recommended Immediate Next Tasks

1. Extend `arxiv_oracle` with page count and persistent text/raster artifact
   paths.
2. Implement front matter capture and `\maketitle` output for `article`, `llncs`,
   `IEEEtran`, `revtex4-2`, and `wacv` semantic shims.
3. Change citation rendering so raw citation keys never enter visible PDF text.
4. Add `.bbl` bibliography rendering into internal output.
5. Introduce a minimal `Document IR` crate/module behind the current string
   output path, initially mirroring paragraphs/headings/title blocks only.

This order is intentionally front-loaded with measurable text recovery before
large layout work. It should raise oracle quality quickly while creating the
interfaces needed for real page layout.
