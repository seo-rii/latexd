# Real Rendering Accepted Structure

This document records the accepted first-batch structure for the real-rendering
work. The immediate goal is to prove the `RenderEvent` -> `Document IR` seam,
not page fidelity.

The accepted pipeline is:

```text
TeX VM execution
  -> RenderEvent stream
  -> Document IR builder
  -> Document IR
  -> layout/page builder
  -> PageDisplayList
  -> renderer backend
```

The first batch should keep the existing string-output and PDF path working
while adding event capture and IR goldens. Do not add Skia, serious text
shaping, external asset conversion, or a real page builder in this batch.

## Current Implementation Status

As of 2026-05-11, the first implementation batch is complete:

- `tex-render-model` owns shared provenance, event, IR, display-list skeleton,
  `AuxView`, and JSON golden helper types.
- `tex-vm` keeps legacy string output authoritative by default and can
  optionally capture a first `RenderEvent` stream.
- `tex-layout` builds first-slice `DocumentIr` from events and a read-only
  `AuxView`.
- `latexd` exposes an internal capture path for `RenderEvent -> DocumentIr`
  debugging/tests without replacing the existing PDF path.
- Compact event and IR JSON goldens cover title, author, date, abstract,
  heading, paragraph text, unresolved citation, display math, bibliography, and
  raw fallback.
- Focused provenance tests cover macro-expanded section text, title metadata
  definition spans, and `\maketitle` emission spans.

Still intentionally outside this batch:

- real page layout and page breaking;
- a serious `PageDisplayList` producer;
- Skia or other renderer backend integration;
- renderer-neutral font resolution and shaping;
- external PDF/EPS/SVG asset conversion;
- full arXiv oracle/raster/performance CI gates.

The next implementation step has started with a narrow display-list spike:

- `tex-layout` can now derive text-only `PageDisplayList` pages from
  `DocumentIr`;
- text positioning uses fixed margins, line height, and approximate advances;
- `glyphs` and `clusters` remain absent by design;
- compact integration goldens now cover `RenderEvent -> DocumentIr ->
  PageDisplayList`;
- compact event/IR/display-list goldens use the same `to_pretty_json` helper as
  debug artifact writing;
- `tex-pdf` can render text-only `PageDisplayList` pages into searchable PDF
  text operations without consuming `DocumentIr` directly;
- `latexd` internal captures now return the derived text-only
  `PageDisplayList` pages and display-list PDF bytes as debug/test artifacts;
- the same capture can write `legacy-output.txt`, `events.json`,
  `document-ir.json`, `page-display-list.json`, `display-list-page-{n}.svg`,
  and `display-list.pdf` into a debug artifact directory;
- display-list PDF/SVG debug rendering now supports positioned text runs,
  simple `Rule` rectangles, and `Save`/`Restore` + `ClipRect` scopes;
- display-list PDF/SVG debug rendering now exposes `Image` operations as
  bounded debug placeholders with asset references, not embedded graphics;
- display-list PDF/SVG debug rendering now exposes `LinkAnnotation` operations
  as PDF link annotations and SVG clickable rectangles;
- display-list PDF/SVG debug rendering now exposes `NamedDestination`
  operations through PDF named destinations and SVG destination markers;
- display-list SVG text elements include primary source attributes plus related
  source roles and span identifiers for source-sync inspection;
- display-list SVG text elements also include bounded expansion stack depth,
  command names, call spans, and definition spans for macro provenance
  inspection;
- this is a renderer-boundary test artifact, not final TeX page layout.

The most important guardrail is:

```text
tex-vm may emit high-level RenderEvents.
tex-vm must not build or mutate Document IR.
```

## Bottom-Line Decisions

- Create `crates/tex-render-model` now.
- Keep `tex-render-model` data-first and dependency-light.
- Keep the first `DocumentIrBuilder` outside `tex-vm`.
- Dual-write legacy string output and optional `RenderEvent` capture during the
  migration phase.
- Treat events as authoritative only for commands that have migrated tests.
- Keep full arXiv oracle, raster diff, Skia, and performance sweeps outside
  default CI.

Recommended first-batch pipeline:

```text
tex-vm
  legacy string output      // preserved
  optional RenderEvent sink // new

latexd / tex-layout-side experiment
  RenderEvent[]
    -> DocumentIrBuilder
    -> Document IR golden tests

existing tex-layout/tex-pdf path
  unchanged for now
```

## New Model Crate

Add one shared crate:

```text
crates/tex-render-model
```

It owns shared data types only:

- `SourceProvenance`;
- `RenderEvent`;
- `Document IR`;
- `PageDisplayList` skeleton;
- `AuxView`-facing view types;
- JSON golden helpers.

It must not own:

- TeX VM execution;
- compiler orchestration;
- renderer backends;
- Skia integration;
- Ghostscript integration;
- full layout algorithms;
- large asset conversion logic.

Recommended dependency shape:

```text
tex-render-model
  -> serde
  -> serde_json
  -> camino
  -> small scalar/hash helpers only

tex-vm
  -> tex-render-model

tex-aux
  -> tex-render-model        // implements/provides AuxView adapters

tex-layout
  -> tex-render-model        // first DocumentIrBuilder can live here initially

tex-pdf
  -> tex-render-model        // later consumes PageDisplayList

latexd
  -> all orchestration crates
```

Initial module structure:

```text
crates/tex-render-model/
  src/lib.rs
  src/provenance.rs
  src/events.rs
  src/ir.rs
  src/display_list.rs
  src/aux_view.rs
  src/golden.rs
```

Do not split immediately into `tex-events`, `tex-ir`, and
`tex-display-list`. Split later only if one module becomes independently large.

Keep the `DocumentIrBuilder` implementation out of `tex-render-model` unless it
stays tiny and pure. A good first location is:

```text
crates/tex-layout/src/document_ir_builder.rs
```

If that grows, split it later into a dedicated `tex-ir-builder` crate.

Current workspace fit:

- `serde`, `serde_json`, and `camino` already exist as workspace dependencies;
- `tex-vm`, `tex-aux`, `tex-layout`, `tex-pdf`, `tex-render-gs`, and `latexd`
  are already separate crates;
- PR 1 does not require new external dependencies beyond the new workspace
  member;
- the crate split can be introduced without changing the current PDF path.

## Event Schema And Goldens

Use pretty JSON with a top-level stream wrapper:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderEventStream {
    pub schema_version: u32,
    pub case: Option<String>,
    pub events: Vec<RenderEventEnvelope>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderEventEnvelope {
    pub event: RenderEvent,
    pub meta: EventMeta,
}
```

Do not repeat `schema_version` on every event in normal goldens. If event
segments are later persisted independently, each segment can use the same stream
wrapper.

Use tagged JSON variants:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RenderEvent {
    Text(TextEvent),
    Space(SpaceEvent),
    ParagraphBreak(ParagraphBreakEvent),

    SetDocumentMetadata(SetDocumentMetadataEvent),
    FlushTitleBlock(FlushTitleBlockEvent),

    BeginBlock(BeginBlockEvent),
    EndBlock(EndBlockEvent),
    Heading(HeadingEvent),

    InlineCitation(InlineCitationEvent),
    BibliographyItem(BibliographyItemEvent),

    GraphicRef(GraphicRefEvent),
    Caption(CaptionEvent),

    InlineMath(MathSourceEvent),
    DisplayMath(MathSourceEvent),

    RawFallback(RawFallbackEvent),
    Diagnostic(RenderDiagnosticEvent),
}
```

Golden policy:

- Event data must always contain source provenance.
- Semantic goldens may normalize noisy source spans.
- Provenance goldens should assert exact paths, UTF-8 offsets, and expansion
  stack behavior.

Example semantic golden normalization:

```json
{
  "kind": "heading",
  "level": 1,
  "content": [{ "kind": "text", "text": "Intro" }],
  "meta": {
    "source": "<present>",
    "mode_hint": "vertical",
    "confidence": "high",
    "producer": "command"
  }
}
```

## Source Provenance

Persist UTF-8 byte offsets as the canonical source span format. Derive
line/column only for UI and reports.

Use role-tagged related spans instead of an untyped `content_spans` list:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceProvenance {
    pub primary: ProvenanceSpan,
    pub related: Vec<RelatedSourceSpan>,
    pub expansion_stack: Vec<ExpansionFrame>,
    pub generated_by: GeneratedBy,
    pub expansion_stack_truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProvenanceSpan {
    File(SourceSpan),
    Generated(GeneratedSpan),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSpan {
    pub path: Utf8PathBuf,
    pub start_utf8: u32,
    pub end_utf8: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedSpan {
    pub stable_id: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedSourceSpan {
    pub role: SourceSpanRole,
    pub span: ProvenanceSpan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceSpanRole {
    Invocation,
    Argument,
    ArgumentContent,
    Definition,
    EmitSite,
    CitationKey,
    MetadataDefinition,
    SyntheticNumbering,
    FallbackSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpansionFrame {
    pub call_span: ProvenanceSpan,
    pub definition_span: Option<ProvenanceSpan>,
    pub command_name: Option<String>,
}
```

Recommended blame policy:

| Output | Primary span |
| --- | --- |
| Literal body text | Literal token span |
| `\section{Intro}` heading text | Argument content span |
| `\section` synthetic number | Command invocation span |
| `\cite{key}` rendered `[?]` or `[3]` | Citation command invocation span |
| Citation key metadata | Related span with role `CitationKey` |
| `\maketitle` title text | Original `\title{...}` content span |
| `\maketitle` block emission | Related span with role `EmitSite` |
| Shim-generated output | Triggering command span if available, otherwise `GeneratedSpan` |
| `.bbl` item text | `.bbl` file span |
| Fallback placeholder | Unsupported construct invocation or environment span |

For `\maketitle`, distinguish block emission from text content:

```text
TitleBlock node:
  primary/source for block emission: \maketitle invocation

Title text inline:
  primary/source for text: \title{...} content
  related emit site: \maketitle invocation
```

Keep expansion stacks bounded:

```rust
pub const MAX_EXPANSION_FRAMES_IN_EVENT: usize = 16;
```

If deeper, truncate and set `expansion_stack_truncated = true`.

## EventSink Migration

Use dual-write for one migration milestone:

```rust
pub trait EventSink {
    fn emit(&mut self, event: RenderEventEnvelope);
}

pub struct NullEventSink;

impl EventSink for NullEventSink {
    fn emit(&mut self, _event: RenderEventEnvelope) {}
}

pub struct VecEventSink {
    pub events: Vec<RenderEventEnvelope>,
}

impl EventSink for VecEventSink {
    fn emit(&mut self, event: RenderEventEnvelope) {
        self.events.push(event);
    }
}
```

The VM can expose either an optional sink:

```rust
pub struct VmOptions<'a> {
    pub event_sink: Option<&'a mut dyn EventSink>,
    // existing options...
}
```

or a simpler capture mode if lifetimes become noisy:

```rust
pub enum EventCapture {
    Disabled,
    Vec,
}
```

Migration rule:

- Legacy string output remains authoritative for existing smoke tests.
- `RenderEvent` output is authoritative for migrated rendering semantics.
- Do not require global equivalence between string output and event output.
- Migrated commands must have event and IR goldens.
- Unmigrated commands may still rely on legacy string output, but should emit
  `RawFallback` or safe text where possible.

Useful test/documentation convention:

```rust
enum MigrationStatus {
    LegacyOnly,
    DualWritten,
    EventAuthoritative,
}
```

This does not need to be runtime state in the first implementation.

## AuxView Contract

Use a narrow read-only trait. The IR builder must not depend on `tex-aux`
internals:

```rust
pub trait AuxView {
    fn citation_label(
        &self,
        key: &str,
        style: CitationStyleHint,
    ) -> Option<CitationLabel>;

    fn bibliography_record(
        &self,
        key: &str,
    ) -> Option<BibliographyRecordView>;

    fn label_target(
        &self,
        key: &str,
    ) -> Option<LabelTargetView>;
}
```

First-slice citation event:

```rust
pub struct InlineCitationEvent {
    pub keys: Vec<String>,
    pub command: String,
    pub style_hint: CitationStyleHint,
}
```

First-slice IR node:

```rust
pub struct CitationInline {
    pub keys: Vec<String>,
    pub style_hint: CitationStyleHint,
    pub resolved_label: Option<String>,
    pub display_text: String,
    pub source: SourceProvenance,
}
```

First-slice rendering policy:

- resolved numeric labels render as `[1]` or `[1,2]`;
- unresolved citations render as `[?]`;
- author-year intent is preserved, but renders as `[?]` unless an already
  formatted label exists;
- raw citation keys must never render as ordinary body text.

If `.bbl` semantic scan and VM bibliography events disagree:

- visible bibliography order: event stream wins;
- labels and metadata: `AuxView` may fill missing data;
- mismatch: add a diagnostic event or low-confidence flag.

Do not attempt citation ranges, author-year grammar, natbib fidelity, or
biblatex fidelity in the first batch.

## Fallback Contract

Use explicit, bounded `RawFallback` events:

```rust
pub struct RawFallbackEvent {
    pub source_excerpt: String,
    pub expanded_text: Option<String>,
    pub normalized_visible_text: Option<String>,
    pub environment: Option<String>,
    pub reason: FallbackReason,
    pub source_hash: Option<String>,
    pub full_source_artifact: Option<String>,
    pub truncated: bool,
}
```

First limits:

- event `source_excerpt`: 2 KiB UTF-8, boundary-safe;
- full fallback source: optional debug artifact only;
- goldens include excerpt and truncation flag;
- goldens do not include huge raw source.

Fallback rendering policy:

| Case | Visible output |
| --- | --- |
| Unknown readable text command | Normalized visible text |
| Unknown huge/noisy environment | Bounded placeholder |
| Unsupported figure with caption | Preserve caption separately; placeholder for image/body |
| Unknown citation command | Citation placeholder, not raw key |
| Unknown math | Math fallback node with raw math source |
| TikZ/PGF | Placeholder plus debug artifact; do not dump source into body |
| Complex table | Monospace/normalized fallback if readable, otherwise placeholder |

Example visible strings:

```text
[unsupported tikzpicture]
[unsupported table: complex tabular]
[missing image: figure.pdf]
```

Invariant:

```text
Unsupported visible material must not silently disappear.
Raw TeX source is not automatically good rendered content.
```

## Display List And Font Staging

The first event/IR vertical slice does not need a serious display list or font
system.

For the first display-list spike, allow approximate text metrics:

```rust
pub struct PositionedTextRun {
    pub origin: Point,
    pub text: String,
    pub font: FontRequest,
    pub size_pt: f32,
    pub approximate_advance_pt: f32,
    pub glyphs: Option<Vec<PositionedGlyph>>,
    pub clusters: Option<Vec<TextCluster>>,
    pub source: SourceProvenance,
}
```

Minimum display-list skeleton:

```rust
pub struct PageDisplayList {
    pub page_id: PageId,
    pub width_pt: f32,
    pub height_pt: f32,
    pub ops: Vec<DrawOp>,
    pub source_spans: Vec<SourceSpan>,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DrawOp {
    Save,
    Restore,
    ClipRect(Rect),
    TextRun(PositionedTextRun),
    Rule(Rect),
    Image(PositionedImage),
    LinkAnnotation(LinkAnnotation),
    NamedDestination(Destination),
}
```

First layout spike policy:

- `glyphs` are optional;
- `clusters` are optional;
- a full font resolver is not required;
- approximate metrics are allowed;
- page-count and raster gates must not treat approximate metrics as final.

Before Skia becomes a serious backend:

- glyph ids are required for shaped text runs;
- clusters are required for searchable PDF and source sync;
- a renderer-neutral font resolver is required;
- a renderer-neutral shaping adapter is required;
- external PDF/EPS interpretation stays in Ghostscript/Poppler or a dedicated
  conversion path.

Staged font model:

```rust
pub struct FontRequest {
    pub family: FontFamilyRequest,
    pub series: FontSeries,
    pub shape: FontShape,
    pub size_pt: f32,
    pub role: FontRole,
}
```

Later:

```text
TexFontRequest
  -> ResolvedFontFace
  -> FontInstance
  -> ShapedTextRun
  -> PageDisplayList
```

## CI Gates

Default CI should include:

- `cargo test -q`;
- `tex-render-model` serialization tests;
- source provenance helper tests;
- event constructor tests;
- IR builder behavior tests;
- small event goldens;
- small IR goldens;
- source-provenance goldens;
- compact reduced fixtures for title, abstract, citation, bibliography, math,
  and fallback.

Default CI should not require:

- local arXiv corpus;
- full arXiv oracle;
- Ghostscript raster diff;
- Skia;
- large image conversion;
- nightly performance sweeps.

Move full corpus work behind ignored, nightly, or manual jobs:

```text
cargo test -p latexd --test arxiv_oracle -- --ignored --nocapture
```

Recommended split:

| Test layer | Default CI | Nightly/manual |
| --- | --- | --- |
| Event unit tests | Yes | Yes |
| Event goldens | Yes | Yes |
| IR builder tests | Yes | Yes |
| IR goldens | Yes | Yes |
| Compact smoke fixtures | Yes | Yes |
| Full arXiv smoke/oracle | No | Yes |
| PDF text oracle | Small only | Full |
| Raster smoke | No or tiny | Yes |
| Skia backend | No | Optional job |
| Performance/cache sweeps | No | Yes |

Default CI catches boundary regressions quickly. Nightly/manual CI catches corpus
compatibility drift.

## Concrete First Implementation Plan

### PR 1: `tex-render-model`

Add:

```text
crates/tex-render-model/
  Cargo.toml
  src/lib.rs
  src/provenance.rs
  src/events.rs
  src/ir.rs
  src/display_list.rs
  src/aux_view.rs
  src/golden.rs
```

Workspace update:

```toml
[workspace]
members = [
  # existing...
  "crates/tex-render-model",
]
```

Dependencies:

```toml
[dependencies]
camino = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
```

Acceptance:

- serde roundtrip tests pass;
- pretty JSON golden helper exists;
- `SourceProvenance` supports file and generated spans;
- `RenderEventStream` has `schema_version = 1`;
- crate has no dependency on `tex-vm`, `tex-layout`, `tex-pdf`, `latexd`, or
  `tex-render-gs`.

### PR 2: Optional VM Event Capture

Add `EventSink` support without changing existing callers.

Initial events:

- `Text`;
- `Space`;
- `ParagraphBreak`;
- `SetDocumentMetadata` for title, author, and date;
- `FlushTitleBlock`;
- `BeginBlock(Abstract)`;
- `EndBlock(Abstract)`;
- `Heading`;
- `InlineCitation`;
- `BibliographyItem`;
- `InlineMath`;
- `DisplayMath`;
- `RawFallback`;
- `Diagnostic`.

Acceptance:

- existing string-output tests still pass;
- event capture is disabled by default;
- compact fixture produces expected event golden.

### PR 3: First `DocumentIrBuilder`

The builder consumes events and an `AuxView`.

First responsibilities:

- group text into paragraphs;
- build `TitleBlock` from metadata and `FlushTitleBlock`;
- build abstract blocks;
- emit heading blocks;
- turn `InlineCitation` into citation inline nodes;
- turn `BibliographyItem` events into bibliography block/items;
- represent math as raw source;
- represent unsupported material as `RawFallback`.

Acceptance:

- IR golden for compact paper fixture passes;
- raw citation key does not appear in paragraph text;
- title, author, abstract, and heading appear in IR text extraction;
- unresolved citation renders `[?]`;
- simple bibliography item appears as bibliography structure.

### PR 4: Source Provenance Focused Tests

Add tiny fixtures for macro expansion and title emission:

```tex
\newcommand{\mysection}[1]{\section{#1}}
\mysection{Intro}
```

```tex
\title{A Paper}
\begin{document}
\maketitle
\end{document}
```

Acceptance:

- section text primary span points to argument content;
- expansion stack includes macro invocation and definition;
- title text primary span points to `\title` content;
- `TitleBlock` emission records `\maketitle` span.

### PR 5: `latexd` Integration Wiring

Add an internal-only command/test path that captures events, builds IR, derives
text-only display lists, and emits a display-list PDF artifact.

Do not replace the PDF path yet.

Acceptance:

- legacy internal PDF path still works;
- event/IR/display-list/PDF artifacts can be written for debugging;
- compact smoke fixture has event, IR, and display-list goldens.
- debug artifact writing covers legacy text, event JSON, IR JSON, display-list
  JSON, per-page display-list SVG, and text-only display-list PDF bytes.

## Recommended Compact Fixture

```tex
\title{A Paper}
\author{Ada Lovelace}
\date{May 1843}

\begin{document}
\maketitle

\begin{abstract}
Short abstract.
\end{abstract}

\section{Intro}
Hello \cite{key}.

\[
  x^2
\]

\begin{thebibliography}{1}
\bibitem{key} Author. Title.
\end{thebibliography}

\begin{unknownenv}
Fallback text.
\end{unknownenv}
\end{document}
```

Expected simplified event sequence:

```text
SetDocumentMetadata(title = "A Paper")
SetDocumentMetadata(author = "Ada Lovelace")
SetDocumentMetadata(date = "May 1843")
FlushTitleBlock
BeginBlock(Abstract)
Text("Short abstract.")
EndBlock(Abstract)
Heading(level = 1, "Intro")
Text("Hello")
Space
InlineCitation(keys = ["key"], command = "cite")
Text(".")
DisplayMath(raw_source = "x^2")
BeginBlock(Bibliography)
BibliographyItem(key = "key", text = "Author. Title.")
EndBlock(Bibliography)
RawFallback(environment = "unknownenv", ...)
```

Expected simplified IR:

```text
TitleBlock(
  title = "A Paper",
  authors = ["Ada Lovelace"],
  date = "May 1843"
)

Abstract("Short abstract.")

Heading(level = 1, content = "Intro")

Paragraph([
  Text("Hello "),
  Citation(keys = ["key"], display_text = "[?]"),
  Text(".")
])

DisplayMath(raw_source = "x^2")

Bibliography([
  Item(key = "key", content = "Author. Title.")
])

RawFallback(...)
```

## Remaining Design Work

The first implementation batch is specific enough to start. The remaining
decisions should be deferred until the event/IR seam is proven:

- exact `EventSink` API shape: borrowed sink vs returned capture mode;
- exact golden normalization helper API;
- exact `DocumentIrBuilder` crate split if `tex-layout` becomes semantically
  awkward;
- exact `AuxView` adapter implementation in `tex-aux`;
- exact compact fixture file layout and naming;
- source-span conversion from existing token spans into UTF-8 byte offsets;
- first debug artifact format for full fallback source;
- first display-list golden format after IR tests stabilize;
- CI workflow split for nightly/manual corpus checks.

These are implementation-shaping decisions, not blockers for PR 1. Skia,
font shaping, external PDF/EPS asset handling, and strict raster diff remain
intentionally out of scope until after the `RenderEvent` and `Document IR`
contracts are testable.
