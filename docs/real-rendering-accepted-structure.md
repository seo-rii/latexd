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

As of 2026-05-13, the first implementation batch is complete:

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
- `abstract*` and `onecolabstract` now follow the same semantic abstract
  event/IR path as `abstract` instead of falling back to unsupported raw text.
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
- `GraphicRef` and `Caption` events now survive semantic recovery as `Graphic`
  IR blocks and derive `Image` display-list operations with caption text;
- `Graphic` IR blocks retain caption provenance separately from image command
  provenance, and display-list caption text uses the caption source span;
- VM render-event capture now emits `GraphicRef`/`Caption` events for
  `\includegraphics`/`\includegraphics*`, `\caption`, and `\caption*`,
  including inside `figure`/`figure*`, `sidewaysfigure`/`sidewaysfigure*`,
  `wrapfigure`/`wrapfigure*`, `table`/`table*`,
  `sidewaystable`/`sidewaystable*`, `wraptable`/`wraptable*`, and
  `SCfigure`/`SCtable` blocks;
- command-style subfigures such as `\subfloat[caption]{...}` and
  `\subcaptionbox{caption}[layout]{...}` now preserve nested graphics and
  captions without leaking layout arguments or raw citation keys;
- captured caption text now redacts nested citation/reference commands to
  placeholders instead of leaking raw citation keys or label keys into
  `Graphic` IR captions and display-list text;
- VM render-event capture now emits `InlineMath` for `\(...\)`/`$...$` and
  `DisplayMath` for `\[...\]`/`$$...$$` plus common display math environments
  such as `equation`, `align`, `flalign`, `alignat`, and `eqnarray`,
  preserving math commands in `raw_source`;
- label definitions inside display math environments, `\[...\]`, and `$$...$$`
  delimiters now emit `LabelDefinition` events while `\label{...}` commands are
  stripped from display math `raw_source`;
- `subequations` now acts as a structured wrapper so inner display math
  environments are captured instead of being swallowed by RawFallback;
- `appendices` and `subappendices` now act as structured wrappers so inner
  headings, references, and body text are captured instead of being swallowed
  by RawFallback;
- VM render-event capture now emits `Heading` levels for `part`/`chapter`,
  `section`, `subsection`, `subsubsection`, `paragraph`, and `subparagraph`,
  preserving the long title span when an optional short title is present;
- captured heading text now uses the same inline citation/reference
  placeholder redaction as captions, so section titles do not leak raw keys
  into IR or display-list text;
- VM render-event capture now emits `InlineCitation` events for common natbib
  and biblatex citation variants such as `citep`, `citet`, `parencite`, and
  `textcite`, skipping optional pre/post notes and preserving citation keys;
- capitalized natbib aliases such as `Citealt` and `Citealp` are handled as
  citation events with the same textual/parenthetical style hints as their
  lowercase counterparts;
- metadata-style citation aliases such as `Citeauthor`, `Citeyear`,
  `Citeyearpar`, `citetitle`/`Citetitle`, and
  `citefullauthor`/`Citefullauthor` also emit citation events instead of
  leaking raw keys;
- identifier/date citation aliases such as `citedoi`, `citeeprint`,
  `citeisbn`, `citeissn`, `citeurl`, `citenum`, `citedate`/`Citedate`, and
  `citeurldate`/`Citeurldate` also emit citation events without exposing keys;
- entry/alias citation commands such as `onlinecite`, `smartcite`,
  `fullcite`, `footfullcite`, `bibentry`, `citetalias`, `citepalias`, and
  `Citetalias` also emit citation events without exposing keys;
- `citefield{key}{field}` now emits citation intent from the key argument and
  consumes the field selector without leaking either argument into visible text;
- multi-cite commands such as `textcites`, `parencites`, and `smartcites`
  consume per-cite options and emit one citation event containing all cited
  keys instead of leaking option or key text;
- `citetext{...}` is treated as visible citation text, preserving nested
  citation events while avoiding raw command, brace, or key leakage;
- `defcitealias{key}{alias}` definitions are consumed as non-visible citation
  metadata so alias keys and replacement text do not leak into body output;
- `addbibresource[...]{...}` is consumed as non-visible bibliography metadata
  so resource paths and options do not leak into rendered body text;
- `printbibliography[...]` now emits an empty bibliography block boundary while
  consuming options without leaking raw biblatex command text;
- legacy BibTeX `bibliographystyle{...}` is consumed as non-visible metadata,
  and `bibliography{...}` emits an empty bibliography block boundary without
  leaking style or database names;
- `nocite{...}` is consumed as non-visible citation-inclusion metadata for now,
  avoiding hidden key or wildcard leakage into rendered text;
- bibliography item text now preserves visible punctuation from common biblatex
  formatting wrappers such as `mkbibquote`, `mkbibparens`, `mkbibbrackets`,
  and `mkbibbraces`;
- `bibinfo{field}{value}` and `bibfield{field}{value}` now render the visible
  value while hiding bibliography field names;
- `captionof{type}[short]{long}` and `captionof*{type}{long}` now emit the
  long visible caption without leaking float type, short-title, or following
  label keys into body text;
- `phantom`, `hphantom`, and `vphantom` now consume their invisible box text
  without leaking that content into bibliography/body extraction;
- TeX spacing control symbols such as `\!`, `\,`, `\;`, `\:`, `\space`,
  and control-space now normalize to invisible or space text instead of
  literal punctuation;
- common no-argument text symbol commands such as `\textquotesingle`,
  `\textquotedbl`, `\textless`, `\textgreater`, `\textbar`, and `\slash` now
  render as their visible symbols;
- bibliography text extraction now passes through common case, text-style, and
  box wrappers while dropping non-visible wrapper options and control commands;
- `\urlstyle{...}` is treated as a non-visible URL style declaration while
  preserving the visible URL from `\url{...}`;
- common bibliography string wrappers now normalize `\bibstring{andothers}`
  to visible `et al` text instead of leaking the raw bibstring key;
- common bibliography punctuation helpers such as `\addcomma`, `\addcolon`,
  `\addsemicolon`, `\adddot`, `\addspace`, `\newunit`, and `\finentry` now
  render visible punctuation without leaking helper command names;
- `\mkbibsuperscript{...}` and `\mkbibsubscript{...}` now attach their visible
  text to the preceding run instead of inserting artificial interword spaces;
- low-level bibliography helpers such as `\adddotspace`, `\isdot`,
  `\bibopenparen`/`\bibcloseparen`, `\bibopenbracket`/`\bibclosebracket`, and
  `\bibopenbrace`/`\bibclosebrace` now render visible punctuation/delimiters;
- bibliography dash and slash helpers such as `\bibrangedash`, `\addslash`,
  `\addhyphen`, `\textendash`, and `\textemdash` now render visible
  punctuation with the expected attachment spacing;
- bibliography spacing helpers such as `\addabbrvspace`, `\addnbspace`, and
  `\addthinspace` are consumed as non-visible separators, while
  `\parentext{...}` renders parenthesized visible text;
- `\bibnamedash` now renders as `---`, and `\urlprefix` is consumed without
  leaking before visible `\url{...}` text;
- VM render-event capture now emits `InlineReference` events for `ref`,
  `eqref`, `pageref`, `autoref`, `nameref`, `cref`/`Cref`, and common
  one-argument aliases such as `subref`, `vref`, `fullref`, `namecref`, and
  `labelcref`, rendering unresolved references as placeholders instead of
  leaking raw label keys;
- page/name/title aliases such as `cpageref`, `Cpageref`, `autopageref`,
  `labelcpageref`, `Fullref`, `titleref`/`Titleref`, `nameCref`,
  `lcnamecref`, `namecrefs`, `nameCrefs`, and `lcnamecrefs` also emit
  reference events and placeholder text without exposing labels;
- theorem/subequation-style commands such as `thmref`, `Thmref`, and
  `subeqref` also emit reference events instead of leaking labels;
- range references such as `crefrange`/`Crefrange`,
  `cpagerefrange`/`Cpagerefrange`, `pagerefrange`, `vpagerefrange`,
  `vrefrange`, and `Vrefrange` now emit a single reference event with both
  endpoint labels instead of exposing either label in visible text;
- VM render-event capture now emits `LabelDefinition` events for `\label{...}`,
  and `DocumentIr` preserves labels as invisible metadata rather than body text;
- `SemanticAux`-backed citation labels and reference targets now have latexd
  integration coverage through `RenderEvent -> DocumentIr -> PageDisplayList`;
- `SemanticAux` citation labels now normalize natbib year-suffix markup such as
  `\natexlab{a}` and `\NAT@exlab{b}` before IR/display-list citation text is
  built, preventing raw bibliography label macros from leaking into output;
- `BibliographyItem` text now uses inline citation/reference placeholder
  redaction before becoming bibliography IR/display-list text, so bibliography
  bodies do not leak nested citation or label keys;
- VM text capture preserves trailing interword spaces before migrated inline
  commands so citations/references do not merge into preceding words;
- `PageDisplayList` text generation preserves inline node provenance by
  emitting positioned text runs for citation, reference, math, and fallback
  segments instead of flattening an entire paragraph under the block source;
- `TitleBlock` now keeps field-level provenance for title, author, and date
  values so display-list text runs can point to the original metadata
  definitions while the block emission still points to `\maketitle`;
- metadata values such as `\title{...}` now use inline citation/reference
  placeholder redaction before becoming `TitleBlock` text, preventing raw keys
  from leaking into extracted or display-list text;
- authblk-style `\author[...]{...}` and `\affil[...]{...}` front matter now
  survives metadata capture; affiliation lines are preserved as title-block
  author lines until a dedicated affiliation IR field exists, and `\thanks`
  text is separated instead of being concatenated into the author name;
- `\footnote{...}` and `\footnotetext[...]{...}` now preserve their body text
  through the same nested inline-event path as text wrappers, so citation and
  reference placeholders survive without leaking raw braces or optional marks;
- top-level one-argument wrapper macros declared with `\newcommand`-style
  definitions and a visible `#1` body now preserve the expanded readable text
  in RenderEvents/IR; this covers author-note/color wrappers such as
  `\newcommand{\note}[1]{{\color{red}[TODO: #1]}}` without leaking the wrapper
  command, color name, braces, or raw citation/reference keys;
- direct color decoration commands such as `\color`, `\textcolor`,
  `\colorbox`, and `\fcolorbox` now hide color/style arguments while preserving
  visible body text with citation/reference placeholder redaction;
- mounted `\input`/`\include` files, in both braced and unbraced forms, now
  enter the RenderEvent/IR path as document fragments, so arXiv-style split
  body files can contribute headings, paragraphs, citations, and references
  instead of leaking only the input filename. Scanner state for declared
  section/wrapper/environment rules is shared with included files, including
  preamble macro files that are loaded before `\begin{document}` and local
  `.sty` files loaded through `\usepackage`/`\RequirePackage` and local `.cls`
  files loaded through `\documentclass`/`\LoadClass`. Conditional file probes
  through `\IfFileExists`/`\InputIfFileExists` scan only the selected branch,
  and `\InputIfFileExists` scans the found file before its success branch.
  Active input stacks now skip cyclic `\input`/`\include` recursion with a
  diagnostic instead of repeatedly rendering the same file. `\includeonly` is
  also honored so non-selected `\include` files do not appear in derived
  IR/display lists, while missing input/include/package/class files produce
  RenderEvent diagnostics instead of silently disappearing;
- `\href` and `\url` now survive as inline link events, `Link` IR nodes, text
  runs, and `LinkAnnotation` display-list operations without leaking hidden
  `\href` targets into visible body text; `\url` supports both braced and
  delimiter-form arguments in event capture;
- `\href{target}{visible}` visible text now uses inline citation/reference
  placeholder redaction, keeping the link target annotation while hiding raw
  citation and label keys from extracted/display-list text;
- structured text normalization for captions, headings, metadata, bibliography,
  and fallback text now treats nested `\href{target}{visible}` as visible text
  only, preventing hidden targets from being concatenated into rendered text;
- the same structured text path now normalizes URL-like wrappers such as
  `\url|...|`, `\path|...|`, `\nolinkurl|...|`, and `\detokenize{...}` as
  wrapper-free visible text, preserving detokenized backslashes;
- `\hyperref[ref]{visible}`, `\hyperlink{target}{visible}`, and
  `\hypertarget{target}{visible}` now preserve only the visible argument in
  body and structured text, keeping labels and anchors out of rendered text;
- starred reference commands such as `\ref*`, `\eqref*`, `\autoref*`,
  `\nameref*`, and `\Cref*` now emit normal reference intent without leaking
  the hidden label into body text, and starred range commands such as
  `\crefrange*` follow the same path;
- non-link text wrappers such as `\nolinkurl`, `\path`, and `\detokenize`
  now survive as visible text events and IR text nodes without creating link
  annotations; URL-like wrappers support both braced and delimiter-form
  arguments where LaTeX commonly permits them;
- simple one-argument text wrappers such as `\emph`, `\textbf`, `\textit`,
  and `\texttt` now preserve their visible text without leaking raw braces
  into the event stream or derived IR;
- text wrappers now preserve nested migrated inline citation/reference events,
  including starred commands such as `\emph{...\citep*{...}}` and
  `\textbf{...\ref*{...}}`, so derived IR and display-list text use
  placeholders/resolved labels instead of leaking raw keys or wrapper braces;
- text wrappers also preserve nested `\href`/`\url` events and URL-like text
  wrappers, keeping hidden link targets out of visible text while still
  deriving display-list link annotations;
- text wrappers also preserve nested inline math delimiters such as
  `\emph{$x^2$}` and `\textbf{\(...\)}` as math events/IR nodes instead of
  leaking raw delimiter syntax into visible text;
- text wrappers now preserve simple text-wrapper commands nested inside other
  text wrappers, so `\emph{...\textbf{...}}` keeps visible text without exposing
  inner wrapper braces;
- text wrappers now preserve readable one-argument unknown commands inside
  wrapper arguments, so `\emph{...\unknown{text}}` keeps normalized visible text
  instead of leaking command names or braces;
- readable unknown commands inside text wrappers now also preserve nested
  citation/reference events, including starred reference/range forms, so
  `\emph{...\unknown{\cite{key}}}` reaches IR as structured
  citation/reference placeholders instead of raw keys or braces;
- readable unknown commands inside text wrappers also preserve nested
  `\href`/`\url` link events and raw math sources, so hidden targets and math
  delimiters do not leak into visible IR/display-list text;
- readable unknown commands inside text wrappers also preserve escaped visible
  characters such as `\%`, `\&`, `\$`, `\_`, `\#`, `\{`, and `\}` instead of
  dropping them while consuming the wrapper command argument;
- readable unknown commands inside text wrappers also preserve simple nested
  text-wrapper commands such as `\textbf{...}` without leaking command names or
  raw braces into derived IR/display-list text;
- readable unknown commands inside text wrappers also preserve readable nested
  one-argument unknown commands, so `\unknown{...\inner{...}}` keeps local
  visible text without leaking the inner command name or braces;
- readable nested unknown commands inside text wrappers now preserve migrated
  citation/reference events, including starred reference/range forms, without
  leaking their raw keys or the nested command's braced argument delimiters;
- readable nested unknown commands inside text wrappers now also preserve
  `\href`/`\url` link events, keeping hidden href targets out of visible
  extracted/display-list text while retaining link annotations;
- readable nested unknown commands inside text wrappers now preserve
  `\nolinkurl`/`\path`/`\detokenize` as visible text even when nested another
  level down, without turning them into link annotations;
- label definitions inside nested text wrappers, including readable nested
  unknown wrappers, now survive as `LabelDefinition` events/IR labels without
  leaking label keys into extracted or display-list text;
- label definitions inside `figure` and `table` environment bodies now survive
  the graphic/caption scan path without leaking float label keys into visible
  text;
- table captions now stay in table context and no longer overwrite the caption
  on a preceding figure graphic during Document IR construction;
- unsupported-environment `RawFallback.normalized_visible_text` now uses inline
  citation/reference placeholder redaction, so fallback text remains readable
  without exposing raw citation or label keys;
- unsupported TikZ/PGF picture environments now render a bounded placeholder
  as visible fallback text while preserving source excerpts for diagnostics;
- escaped visible characters such as `\%`, `\&`, `\$`, `\_`, `\#`, `\{`,
  and `\}` now survive as text events instead of disappearing during capture;
- nonbreaking `~` spaces now survive as explicit visible spaces in event
  capture and normalized text instead of leaking literal tildes into IR text;
- explicit `\\` line breaks now survive as `LineBreak` events, IR inline nodes,
  and display-list line advances rather than silently merging adjacent text;
- `itemize`, `enumerate`, and `description` now survive as list block events,
  `List` IR blocks, and display-list text runs with default or explicit item
  markers while preserving inline events inside list item content;
- simple text/theorem environments such as `quote`, `quotation`, `verse`,
  `center`, `flushleft`, `flushright`, `theorem`, `proof`, `lemma`,
  `proposition`, `corollary`, `definition`, `remark`, and `example` now
  survive as structured environment block events and IR blocks instead of
  falling back to unsupported-environment raw text;
- acknowledgements environment variants now also survive as structured
  environment block events and IR blocks while preserving inline citations;
- keywords environment variants now survive as structured environment block
  events and IR blocks while preserving inline citations;
- `frontmatter` now acts as a structured wrapper so title/author/date metadata,
  abstract content, and body text are captured instead of being swallowed by
  RawFallback;
- `widetext`, `strip`, and `fullwidth` now act as structured wrappers so their
  body text and inline events are captured instead of being swallowed by
  RawFallback;
- `landscape` now acts as a structured wrapper, with `lscape.sty` and
  `pdflscape.sty` available through builtin package shims until page-orientation
  layout is modeled;
- `rotating.sty` and `sidecap.sty` now resolve through builtin package shims,
  with `sidewaysfigure`/`sidewaystable` and `SCfigure`/`SCtable` preserving
  captions, graphics, and labels without RawFallback;
- `CJK` and `CJK*` now act as structured wrappers, with `CJK.sty` and
  `CJKutf8.sty` available through builtin package shims and encoding/font
  arguments consumed before body capture;
- `sloppypar` now acts as a structured wrapper so line-breaking hints do not
  hide body text behind RawFallback;
- font-size declaration environments such as `small`, `footnotesize`, and
  `Large` now act as structured wrappers so style hints do not hide body text;
- `samepage` now acts as a structured wrapper so page-break hints do not hide
  body text behind RawFallback;
- `titlepage` now acts as a structured wrapper so title-page content is
  preserved until dedicated title-page layout exists;
- `NoHyper` now acts as a structured wrapper while preserving visible `href` and
  `url` text without producing link annotations inside the suppressed region;
- boxed emphasis environments such as `framed`, `shaded`, `tcolorbox`, and
  `mdframed` now act as structured wrappers, with common style options consumed
  before visible body text is captured;
- `csquotes` display environments `displayquote` and `displayquotation` now act
  as structured wrappers while consuming optional attribution/punctuation
  arguments before body capture;
- `comment` environment bodies are now skipped during render-event capture so
  non-visible notes do not leak into Document IR or display-list text;
- `\excludecomment{...}` and `\includecomment{...}` now update render-event
  capture policy for custom comment-package environments, and `comment.sty`
  resolves through the builtin package shim surface;
- `lineno.sty` now resolves through the builtin package shim surface, and
  line-numbering commands such as `\linenumbers`, `\modulolinenumbers`, and
  `\resetlinenumber` are consumed without visible text leakage;
- `siunitx.sty` now resolves through the builtin package shim surface, and
  common quantity/number/unit commands such as `\SI`, `\num`, `\si`, and
  `\SIrange` render readable text while hiding raw command syntax and setup
  declarations;
- common layout commands such as `\vspace`, `\hspace`, `\pagebreak`,
  `\smallskip`, and `\noindent` are now consumed as non-visible layout hints so
  dimensions and break options do not leak into rendered text;
- float/layout and TOC helper commands such as `\FloatBarrier`, `\balance`,
  `\flushend`, `\phantomsection`, `\addcontentsline`, and `\addtocontents` are
  now consumed without visible text leakage, while `\xspace` preserves a single
  explicit space;
- `spacing`, `onehalfspace`, `doublespace`, and `singlespace` now act as
  structured wrappers while consuming line-spacing arguments so they do not
  appear as visible text;
- `adjustwidth` and `adjustwidth*` now act as structured wrappers while
  consuming margin arguments so they do not appear as visible text;
- `addmargin` and `addmargin*` now act as structured wrappers while consuming
  optional/required margin arguments so they do not appear as visible text;
- `minipage` now uses the structured environment path while consuming layout
  position/width arguments so they do not appear as visible text;
- `multicols` and `multicols*` now use the structured environment path while
  consuming column-count arguments so they do not appear as visible text;
- `paracol` and `paracol*` now use the same structured environment path while
  `paracol.sty` resolves through the builtin package shim surface;
- `threeparttable`, `measuredfigure`, and `tablenotes` now act as structured
  wrappers, with `tablenotes` options consumed before table-note body capture;
- `subfigure`/`subfigure*` and `subtable`/`subtable*` now act as structured
  wrappers while consuming minipage-like position/size arguments before body
  capture;
- `algorithm`, `algorithm*`, `algorithmic`, and `algorithmic*` now use the
  same structured environment path, preserving captions, labels, and body text
  without emitting RawFallback;
- theorem-like environment optional titles are consumed as visible title text
  without leaking raw square-bracket syntax into IR or display-list text;
- `\newtheorem{...}{...}` and `\newtheorem*{...}{...}` declarations register
  custom theorem-like environment names for the same structured block and
  optional-title path;
- unsupported `tabular`/`longtable`-style environments now normalize column
  specs, cell separators, row breaks, and common rule commands into readable
  fallback text instead of leaking raw table syntax into extracted/display-list
  text;
- labels inside table-style fallback bodies now emit `LabelDefinition` events
  and are removed from visible fallback text;
- unsupported `verbatim` environments now preserve their body text without
  applying LaTeX command normalization, so backslashes, braces, and code-like
  snippets remain visible in IR and display-list fallback text;
- unsupported `lstlisting`, `minted`, and fancyvrb `Verbatim` code
  environments now drop begin-time listing options/language arguments from
  visible fallback text while preserving the raw code body;
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
    InlineReference(InlineReferenceEvent),
    LabelDefinition(LabelDefinitionEvent),
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
