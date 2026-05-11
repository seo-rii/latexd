# Design Question: Where Should Document IR Be Produced?

## Short Question

`latexd` needs to move from a text-scaffold internal PDF to real rendering. The
most important unresolved design decision is the boundary between TeX macro
execution and the future Document IR:

Should the TeX VM produce Document IR directly while executing macros, or should
the VM emit a lower-level stream of semantic/render events that a separate
Document IR builder consumes?

## Context

`latexd` currently has an internal compiler path that can process representative
arXiv papers without falling back to an external LaTeX compiler.

The current pipeline is roughly:

```text
TeX source
  -> tex-lexer
  -> tex-vm expansion/execution
  -> linear output string
  -> tex-layout fixed-width line wrapping
  -> tex-pdf simple text PDF
```

This works as an infrastructure smoke test, but it is not real rendering. The
internal PDF is a flat text approximation. It has no real title layout, no real
paragraph/page builder, no image placement, no table model, no math layout, and
only minimal class/package semantic behavior.

After recent arXiv CC0 oracle work, these six local corpus cases build with zero
diagnostics in strict mode:

| arXiv id | Internal build | Diagnostics | Internal/oracle token count | Unique-token overlap |
| --- | --- | ---: | ---: | ---: |
| `2602.14379` | ok | 0 | 0.861 | 0.598 |
| `2508.10038` | ok | 0 | 0.892 | 0.742 |
| `2404.05196` | ok | 0 | 0.728 | 0.568 |
| `2403.07956` | ok | 0 | 0.579 | 0.535 |
| `2102.03748` | ok | 0 | 0.953 | 0.885 |
| `2302.01837` | ok | 0 | 0.895 | 0.660 |

The low overlap is now mostly a rendering/modeling problem rather than a compile
success problem. Known causes:

- `\title`, `\author`, `\affil`, `\thanks`, and class-specific front matter are
  mostly consumed or flattened;
- citation commands can leak raw citation keys rather than formatted references;
- `.bbl` text is not yet rendered as bibliography structure;
- `\includegraphics` becomes `[image]`;
- math commands are ASCII stubs instead of math glyph/layout;
- class shims such as `llncs`, `IEEEtran`, `revtex4-2`, and `wacv` currently
  suppress complexity more than they preserve visible semantic output;
- layout is fixed-width text wrapping, not a block/page builder.

The proposed real-rendering plan introduces a Document IR:

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
```

The unresolved point is where and how that IR should be created.

## Constraints

The design needs to preserve existing `latexd` strengths:

- incremental preview and page reuse;
- source spans for source-to-preview sync;
- preamble snapshots and checkpoint replay;
- semantic aux artifacts for labels/citations/toc/bibliography;
- local arXiv-style project resolution;
- conservative behavior when TeX/package semantics are unknown.

The design should not require full LaTeX compatibility before visible rendering
improves.

The design must also account for TeX realities:

- macros can redefine rendering commands at any time;
- content can be produced by expansion, conditionals, loops, and class/package
  hooks;
- some visible output depends on side effects collected in the preamble and
  emitted later, such as `\maketitle`;
- source spans can cross macro expansion boundaries;
- class/package shims intentionally approximate behavior for preview;
- unsupported constructs need visible/local fallback, not silent deletion.

## Option A: VM Produces Document IR Directly

In this design, TeX execution directly mutates a `DocumentBuilder` or appends IR
nodes.

Examples:

- `\section{Intro}` appends `Heading { level: 1, ... }`;
- paragraph text appends text into the current `Paragraph`;
- `\maketitle` appends a `TitleBlock`;
- `\includegraphics` appends or updates a `Figure`;
- citation commands append `Citation` inline nodes;
- display math commands append `DisplayMath`.

### Advantages

- simpler first implementation;
- direct access to VM state, current source frame, and macro arguments;
- easier to make front matter and citation fixes quickly;
- existing command handlers can be extended one by one;
- fewer moving parts during early migration from string output.

### Risks

- VM becomes coupled to rendering policy and layout-level concepts;
- replay/checkpoint state must include partially built IR state;
- macro expansion, semantic aux, and rendering effects may become tangled;
- harder to support multiple render targets or alternate IR normalization;
- unknown commands may be difficult to classify cleanly as text, semantic event,
  or layout event;
- future page builder changes may require touching VM internals too often.

## Option B: VM Emits Semantic/Render Events, IR Builder Consumes Them

In this design, the VM does not directly own Document IR. It emits an ordered
event stream, and a separate builder turns events into IR.

Example events:

```text
BeginParagraph(source_span)
Text(text, source_span)
CommandSemantic { name: "section", args, source_span }
InlineCitation { keys, source_span }
GraphicRef { path, options, source_span }
DisplayMathSource { source, source_span }
StoredMetadata { key: "title", value, source_span }
FlushTitleBlock(source_span)
EndParagraph(source_span)
RawFallback { text, reason, source_span }
```

The IR builder owns paragraph grouping, title block construction, figure/caption
association, list nesting, table construction, and fallback policy.

### Advantages

- keeps VM closer to TeX execution and less coupled to layout;
- event stream can be recorded, inspected, and regression-tested;
- easier to replay from checkpoints if event builder state is explicit;
- different builders could generate Document IR, semantic-only reports, or debug
  transcripts;
- class/package semantic shims can emit events without knowing final IR layout;
- fallback policy can be centralized.

### Risks

- more initial architecture;
- event vocabulary can become another poorly specified intermediate language;
- builder needs enough context to group text into paragraphs, figures, tables,
  and lists correctly;
- source span handling across macro expansion may be harder to reason about;
- if events are too low-level, the builder reimplements TeX semantics badly;
- if events are too high-level, this collapses back into direct IR mutation.

## Option C: Hybrid Event Stream With Early IR Hooks

This design uses events as the stable boundary, but allows a small number of
high-confidence commands to produce high-level events immediately.

Examples:

- raw character output becomes `Text`;
- `\section` emits `HeadingEvent`;
- `\maketitle` emits `FlushTitleBlock`;
- `\cite` emits `CitationEvent`;
- `\includegraphics` emits `GraphicEvent`;
- table/math environments initially emit `RawEnvironmentEvent`, then become
  structured later.

The Document IR builder consumes the event stream and owns block construction.

### Advantages

- gives a small first step without over-coupling the VM to layout;
- lets high-value fixes land early: front matter, citations, headings,
  bibliography;
- provides an inspectable contract before real page layout;
- leaves room for table/math/image events to become more structured over time;
- supports conservative fallback.

### Risks

- needs discipline to prevent event vocabulary from drifting;
- requires clear tests that define event ordering and builder behavior;
- some state will still live in the VM until migrated.

## Accepted Decision

Adopt Option C, with one stricter invariant:

```text
TeX VM execution
  -> RenderEvent stream
  -> Document IR builder
  -> Document IR
  -> layout/page builder
  -> PageDisplayList
  -> renderer backend
```

The VM may emit high-level semantic `RenderEvent`s, but it must not mutate
`Document IR` directly. `Document IR` is still a stable internal type, but it is
a derived semantic artifact produced by a deterministic builder rather than the
VM's mutation target.

The core invariant is:

```text
The VM decides what TeX execution produced.
The IR builder decides what document structure that output represents.
The layout engine decides where it goes.
The renderer only draws already-positioned page operations.
```

Use both `RenderEvent` and `Document IR` as stable boundaries, but with different
roles:

- `RenderEvent` is the stable boundary between TeX execution and semantic
  recovery;
- `Document IR` is the stable boundary between semantic recovery and layout;
- `PageDisplayList` is the stable boundary between layout and renderer backends.

This avoids coupling macro execution to layout policy, keeps replay/checkpoint
semantics renderer-neutral, and gives tests inspectable artifacts before PDF or
raster output.

## RenderEvent Contract

The first event vocabulary should be typed and versioned. A good first contract
is:

```rust
enum RenderEvent {
    Text(TextEvent),
    Space(SpaceEvent),
    ParagraphBreak(ParagraphBreakEvent),
    SetDocumentMetadata(MetadataEvent),
    FlushTitleBlock(FlushTitleBlockEvent),
    BeginBlock(BlockKindEvent),
    EndBlock(BlockKindEvent),
    Heading(HeadingEvent),
    InlineCitation(CitationEvent),
    BibliographyItem(BibliographyItemEvent),
    GraphicRef(GraphicRefEvent),
    Caption(CaptionEvent),
    InlineMath(MathSourceEvent),
    DisplayMath(MathSourceEvent),
    RawFallback(RawFallbackEvent),
    Diagnostic(RenderDiagnosticEvent),
}
```

Every event should carry common metadata:

```rust
struct EventMeta {
    event_id: EventId,
    source: SourceProvenance,
    mode_hint: ModeHint,
    confidence: SemanticConfidence,
    producer: EventProducer,
}
```

`producer` should distinguish primitives, macros, class/package shims, `.bbl`
parsers, and fallback paths. `confidence` should record whether the event is
normal semantic output, an approximation, or a conservative fallback.

The `DocumentIrBuilder` should be a mostly deterministic consumer:

```text
(events, aux_view, asset_resolver, builder_options) -> Document IR
```

That makes event golden tests meaningful and lets `latexd` rebuild IR from an
event log or from event segments after a checkpoint.

## Checkpoints And Derived Caches

VM checkpoints should not include partially built rendering state. Keep VM
snapshots about TeX execution state only: input cursor, macro/catcode/register
state, conditionals, aux-relevant state, and enough resolver state to replay
deterministically.

Rendering state should live in derived caches:

```text
VmCheckpoint
  -> EventSegment cache
  -> IrBuilderCheckpoint cache
  -> DocumentIR cache
  -> Layout/PageDisplayList cache
  -> Renderer tile/cache
```

Practical replay rule:

```text
nearest valid VmCheckpoint
  -> replay TeX from there
  -> append/rebuild RenderEvent segment
  -> replay IR builder from nearest valid IrBuilderCheckpoint
  -> rebuild affected DocumentIR/layout/page-display-list regions
```

An `IrBuilderCheckpoint` is acceptable for performance, but it must be
invalidatable derived state keyed by the VM checkpoint and event-prefix hash. It
must not become part of the VM snapshot correctness contract.

Suggested cache keys:

```text
EventSegmentKey {
    vm_checkpoint_id,
    input_range_hash,
    macro_state_hash,
    resolver_hash,
}

DocumentIrKey {
    event_stream_hash,
    aux_sem_hash,
    asset_manifest_hash,
    ir_builder_version,
}

LayoutKey {
    document_ir_hash,
    page_style_hash,
    font_metrics_hash,
    layout_engine_version,
}

PageDisplayListKey {
    layout_page_hash,
    shaped_run_hashes,
    asset_hashes,
    display_list_version,
}

RenderedTileKey {
    backend_id,
    backend_version,
    page_display_list_hash,
    tile_rect,
    scale,
    device_pixel_ratio,
    color_mode,
}
```

Renderer-specific state, decoded images, shaped glyph caches, Skia surfaces, and
rendered tiles do not belong in VM checkpoints.

## Source Provenance

Source spans should be represented as provenance, not as a single span.

```rust
struct SourceProvenance {
    primary: SourceSpan,
    content_spans: Vec<SourceSpan>,
    expansion_stack: Vec<ExpansionFrame>,
    generated_by: GeneratedBy,
}

struct ExpansionFrame {
    call_span: SourceSpan,
    definition_span: Option<SourceSpan>,
    command_name: Option<String>,
}
```

Recommended blame policy:

- literal body text points to the literal token span;
- `\section{Intro}` visible heading text points primarily to the argument span;
- `\cite{key}` rendered as `[3]` points primarily to the invocation span, with
  the key span attached;
- `\maketitle` title text carries title/author content spans plus the
  `\maketitle` flush span;
- synthetic numbering and punctuation point to the generating command;
- shim-generated output records shim provenance and approximation confidence.

For `\title{A}` in the preamble and `\maketitle` later, the `TitleBlock` should
have both an emit span for `\maketitle` and content spans for the stored title,
author, date, and note arguments.

## Semantic Policies

Class/package shims should emit high-level semantic events directly when they
intentionally approximate complex package behavior. Prefer defining TeX macros
only when the real macro flow naturally reaches normal event-producing commands.
Shims must not mutate `Document IR` directly.

Paragraph handling should be hybrid:

- the VM emits paragraph-breaking and mode signals when execution clearly
  reaches them;
- the IR builder owns actual paragraph grouping;
- text opens a paragraph implicitly;
- `ParagraphBreak` closes the current paragraph;
- structural block events close the current paragraph before emitting
  themselves;
- display math closes or suspends the paragraph according to builder policy.

Unsupported visible material must never silently disappear. `RawFallback` should
store source text, optional expanded text, optional normalized visible text,
environment name, fallback reason, and provenance. Render the most readable
bounded local fallback available. Unknown citations should become `[?]`, not raw
body text.

Citations should be resolved in the IR builder from read-only semantic aux and
`.bbl` data. The VM emits citation intent; layout and rendering consume the
chosen citation inline node/text.

`.bbl` handling should produce both semantic records and bibliography events:

```text
.bbl semantic scan -> BibliographyRecord { key, label, raw_text, parsed_text }
thebibliography execution -> BibliographyItem events
IR builder -> Bibliography block
```

Math should initially be raw source plus optional normalized text and a small
math AST. Lossy ASCII text can be a derived metric output, but it should not be
the canonical math model.

## Additional Help Needed: Backend and Layout Boundary

The event/IR boundary is the first decision, but the next difficult design area
is the boundary between Document IR, layout, and the actual renderer. This
becomes more important if `latexd` adopts Skia as a rendering backend.

Skia is attractive as a 2D drawing backend because it can draw text, paths,
images, and pages to raster outputs and can also produce PDF. However, Skia does
not replace TeX's paragraph builder, page builder, math layout, float placement,
or LaTeX package semantics. If used, it should probably consume a page-level
display list or box tree after `latexd` has already made layout decisions.

The design help needed here is not "Should Skia be used at all?" in isolation.
The harder question is what contract should exist between `latexd` layout and
any renderer backend, including Skia.

Open areas that need careful design:

- Renderer input: Should the renderer consume `Document IR`, a TeX-like box
  tree, or a normalized page display list?
- Text shaping: Should text shaping, glyph selection, ligatures, and font
  fallback be owned by `latexd`, by Skia/HarfBuzz, or by a thin adapter?
- Font model: How should TeX font concepts such as TFM metrics, map files,
  encodings, math fonts, and virtual fonts map to renderer fonts?
- PDF text fidelity: If Skia emits PDF, what guarantees do we need for
  searchable/extractable text compared with the current PDF text oracle?
- Raster fidelity: Should raster diffs compare Skia output, Ghostscript output
  from generated PDF, or both?
- External assets: How should `\includegraphics` handle PDF, EPS, SVG, and
  bitmap inputs when Skia is not a full general-purpose PDF/EPS interpreter?
- Incremental preview: What is the cache key for page display lists, shaped
  text runs, decoded images, and rendered tiles?
- Backend portability: Can `tex-pdf` remain a simple fallback while Skia is an
  optional backend, or would maintaining both distort the layout abstraction?
- Build and distribution: Is the Rust Skia dependency acceptable for CI,
  releases, and local development given native build/prebuilt binary complexity?

## Proposed Renderer Boundary

If Skia is introduced, the safer architecture is:

```text
VM execution
  -> RenderEvent stream
  -> Document IR builder
  -> layout/page builder
  -> PageDisplayList
  -> renderer backend
       -> tex-pdf fallback
       -> tex-render-skia PDF/raster
       -> debug/inspection output
```

`PageDisplayList` would be the renderer-facing contract. It should contain
already-positioned text runs, paths, images, and annotations in page coordinates.
The renderer should not decide paragraph breaks, float placement, section
numbering, citation formatting, or math structure. Those decisions belong
earlier in the pipeline.

This keeps Skia useful without letting it become an accidental layout engine.
It also gives tests a stable target before pixel-level rendering:

- event golden tests define TeX execution semantics;
- Document IR golden tests define semantic recovery;
- page display-list golden tests define layout decisions;
- PDF text and raster tests define backend behavior.

Minimum serious `PageDisplayList` model before a Skia backend:

```rust
struct PageDisplayList {
    page_id: PageId,
    width_pt: f32,
    height_pt: f32,
    ops: Vec<DrawOp>,
    source_spans: Vec<SourceSpan>,
    content_hash: Hash,
}

enum DrawOp {
    Save,
    Restore,
    ClipRect(Rect),
    ClipPath(PathId),
    TextRun(PositionedTextRun),
    Rule(Rect),
    Path(PositionedPath),
    Image(PositionedImage),
    LinkAnnotation(LinkAnnotation),
    NamedDestination(Destination),
}
```

Text shaping should happen before final `PageDisplayList` emission through a
renderer-neutral shaping adapter. Skia may implement the adapter, but the final
renderer backend should not own line-breaking, shaping policy, or citation/math
semantics.

`latexd` also needs a renderer-neutral font layer:

```text
TexFontRequest -> ResolvedFontFace -> FontInstance
```

If TeX metrics are available, layout should use them as authority. If outline
metrics are available, renderers should use them for drawing and glyph geometry.
If both are available, layout uses TeX metrics while renderers draw mapped
outlines with compatible scaling. Missing exact fonts should produce fallback
diagnostics and cache keys that reflect the fallback.

Skia should stay optional behind a feature flag until `PageDisplayList`, text
shaping, and font contracts stabilize. Ghostscript/Poppler should remain
available for external PDF/EPS interpretation even if Skia handles final page
rendering.

## Implementation Feasibility

This direction is implementable now as an incremental migration, but only if the
first batch stays deliberately small.

Feasible immediately:

- define `RenderEvent`, `EventMeta`, and `SourceProvenance` types in a new
  internal crate or `latexd` module;
- add an `EventSink` beside the existing VM string output path;
- emit events for text, spaces, paragraph breaks, title metadata,
  `\maketitle`, abstract, headings, citations, `.bbl` items, graphics, captions,
  math source, and raw fallbacks;
- build a first `Document IR` from those events;
- keep existing text/PDF output as a compatibility fallback while the IR path is
  incomplete;
- add event golden tests and IR golden tests before changing page layout.

Still needs focused design before broad implementation:

- exact crate/module ownership for events, IR, layout, and display lists;
- event schema versioning and golden serialization format;
- source provenance data model that fits existing `tex-tokens` spans;
- how the VM exposes mode hints without pretending to implement full TeX
  vertical/horizontal/math mode semantics yet;
- `aux_view` interface for citation and bibliography resolution;
- asset resolver contract for graphics before renderer work begins;
- font and text shaping model before Skia or serious page layout;
- display-list golden format and tolerances;
- CI strategy so long arXiv smoke and optional Skia work do not block every
  default test run.

## Specific Questions

1. What should be the stable boundary type: `Document IR`, `RenderEvent`, or both?

2. Should the VM snapshot/checkpoint include partially built rendering state, or
   should replay rebuild events from the checkpoint boundary?

3. How should source spans be represented when visible output comes from a macro
   defined in one file and invoked in another?

4. Should class/package shims emit high-level semantic events directly, or should
   they define TeX macros that eventually trigger normal event-producing
   commands?

5. How should paragraph boundaries be detected? Should the VM emit paragraph
   start/end events, or should the IR builder infer paragraphs from text and
   vertical-mode-like commands?

6. What is the fallback contract for unsupported environments? Is
   `RawFallback { source_text }` acceptable in output, or should unsupported
   environments preserve expanded text only?

7. How should citation rendering interact with existing semantic aux artifacts?
   Should citation labels be resolved during VM execution, IR building, or layout?

8. Should `.bbl` parsing produce semantic aux data, render events, or IR nodes?

9. Should math be represented initially as raw source, expanded text, or a small
   math AST?

10. What tests should define the boundary first: event golden tests, IR golden
    tests, PDF text tests, or raster tests?

11. Should renderer backends consume a page display list rather than Document IR
    directly?

12. What is the minimum page display-list model needed before adding Skia:
    positioned text runs only, or text plus paths/images/annotations?

13. Should text shaping happen before the display list is produced, or inside
    the Skia backend?

14. How should `latexd` represent fonts so that native PDF output, Skia PDF
    output, and Skia raster output use compatible metrics?

15. Should Ghostscript/Poppler remain the path for interpreting external PDF/EPS
    graphics even if Skia handles final page rendering?

16. What renderer behavior should be considered test-critical: extracted PDF
    text, page count, bounding boxes, raster diff, or all of them at different
    phases?

17. Should Skia be optional behind a feature flag until the page display-list
    contract is stable?

18. How should incremental preview cache page display lists and renderer tiles
    without making renderer-specific state part of VM checkpoints?

## Direct Answers

1. Use both. `RenderEvent` is the VM boundary; `Document IR` is the
   semantic/layout boundary. Add `PageDisplayList` as the renderer boundary.

2. VM snapshots should not include partial rendering state. Replay events from
   VM checkpoints; optionally cache IR-builder checkpoints as derived state.

3. Use `SourceProvenance`: primary invocation/content span plus expansion stack
   and definition spans.

4. Prefer normal macros when they naturally reach event commands. Let shims emit
   high-level events directly when intentionally approximating complex behavior.

5. The VM emits paragraph-breaking/mode signals; the IR builder owns paragraph
   grouping.

6. `RawFallback` is acceptable if it stores raw source, optional expanded text,
   reason, and provenance. Never silently delete visible material.

7. The VM emits citation intent. The IR builder resolves citation labels using
   semantic aux and `.bbl` data.

8. `.bbl` parsing should produce semantic records plus bibliography events. IR
   nodes are built by the IR builder.

9. Initially store raw math source plus optional normalized text and small AST.
   Do not make lossy ASCII the canonical representation.

10. Define event goldens first, IR goldens second, `PageDisplayList` goldens as
    layout begins, PDF text after that, and raster smoke later.

11. Yes. Renderer backends consume `PageDisplayList`, not `Document IR`.

12. Before Skia, define positioned text, rules/rects, images, links, clipping,
    and page/source metadata. Text-only is only enough for a spike.

13. Shape before final display-list emission through a renderer-neutral adapter.

14. Use a neutral font resolver from TeX requests to resolved faces and font
    instances. Layout and renderers must share metrics.

15. Yes. Keep Ghostscript/Poppler for external PDF/EPS interpretation.

16. All are test-critical, but phased: PDF text and page count early, bounding
    boxes and nonblank pages next, raster diff later.

17. Yes. Keep Skia behind a feature flag until display-list and text/font
    contracts stabilize.

18. Cache event segments, IR, layout, display lists, shaped runs, decoded assets,
    and tiles independently. VM checkpoints stay renderer-neutral.

## Suggested First Design Spike

Implement a tiny vertical slice behind a feature flag or internal-only path:

Input:

```tex
\title{A Paper}
\author{Ada Lovelace}
\begin{document}
\maketitle
\begin{abstract}
Short abstract.
\end{abstract}
\section{Intro}
Hello \cite{key}.
\bibliographystyle{plain}
\begin{thebibliography}{1}
\bibitem{key} Author. Title.
\end{thebibliography}
\end{document}
```

Expected event stream:

```text
SetDocumentMetadata(title="A Paper")
SetDocumentMetadata(author="Ada Lovelace")
FlushTitleBlock
BeginBlock(Abstract)
Text("Short abstract.")
EndBlock(Abstract)
Heading(level=1, text="Intro")
Text("Hello ")
InlineCitation(keys=["key"])
Text(".")
BibliographyItem(key="key", text="Author. Title.")
```

Expected IR:

```text
TitleBlock(title="A Paper", authors=["Ada Lovelace"])
Abstract("Short abstract.")
Heading(level=1, "Intro")
Paragraph([Text("Hello "), Citation("key"), Text(".")])
Bibliography([Item("key", "Author. Title.")])
```

Acceptance criteria for the spike:

- raw citation key does not appear as body text;
- title and author appear in extracted internal PDF text;
- source spans point to the invocation site for visible body output;
- macro definitions can still live in separate files;
- existing string-output path still works while the IR path is incomplete.

## Accepted First-Batch Structure

The architecture direction is specific enough for the first event/IR
implementation batch. The accepted near-term structure is:

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

The concrete Rust ownership split is:

- `crates/tex-render-model` owns `RenderEvent`, `SourceProvenance`,
  `Document IR`, the `PageDisplayList` skeleton, `AuxView`-facing view types,
  and JSON golden helpers;
- `tex-vm` emits optional `RenderEvent`s while keeping the legacy string output;
- the first `DocumentIrBuilder` lives outside `tex-vm`, initially under
  `tex-layout` or an equivalent layout-side experiment;
- existing `tex-layout` and `tex-pdf` behavior remains unchanged until event
  and IR goldens are stable.

The accepted structure, first PR sequence, compact fixture, and remaining
deferred decisions are collected in
[`real-rendering-accepted-structure.md`](real-rendering-accepted-structure.md).

The first coding batch should stop after proving the event-to-IR vertical slice
for title, author, abstract, heading, paragraph text, citation placeholders,
basic bibliography item events, raw math source, and raw fallback. Skia and
serious font shaping should wait until `PageDisplayList` is defined and tested.
