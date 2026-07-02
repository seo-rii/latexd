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

Current Phase 3 status: the first table slice is implemented at the event/IR
boundary. Common `tabular`, `tabular*`, `tabularx`, `array`, and `longtable`
bodies are normalized into row/cell `Table` IR and rendered as readable monospaced
display-list text, including table-float captions and basic max-width column
padding for uneven rows. Horizontal table rules from `\hline` and common
booktabs commands are preserved as row rule flags and now emit renderer-visible
`PageDisplayList::Rule` rectangles while preserving dashed separators in the
readable display-list fallback. Common `arydshln` dashed rule commands such as
`\hdashline` and `\cdashline{a-b}` share that coarse rule path without leaking
command names into table text. Simple `\cline{a-b}` / `\cmidrule(...){a-b}`
spans are carried as zero-based inclusive column ranges with whitespace outside
the covered columns and matching partial rule rectangles, using the same visible
separator widths as table row text when trimming the rule span. Simple
`booktabs` `\cmidrule[...](l/r/lr){...}` optional rule widths and trim options
also survive without leaking control payloads into table text; `l{...}` /
`r{...}` length payloads are preserved as point-sized trim hints and shorten the
renderer-visible partial rule rectangle x/width directly.
A default-CI table readability gate now checks that representative tables keep
cell text in display-list/PDF text while rendering horizontal and vertical
separators as `Rule` operations rather than searchable filler glyphs, with
optional `pdftotext` and Ghostscript raster gross checks when those tools are
available.
Simple table environments nested inside a table cell now stay inside that outer
cell as readable flattened text, rather than truncating the outer table at the
inner `\end{tabular}` or leaking the nested `\begin{tabular}{...}` preamble as
body text.
Simple
`\multicolumn{n}{...}{text}` cells are also normalized to visible cell text plus
`TableCell.column_span` metadata so the display-list fallback can occupy the
combined monospaced column width, including visible intercolumn separator widths
inside the span, and simple `l` / `c` / `r` multicolumn specs now survive as
`TableCell.alignment` overrides for readable spanning headers.
Simple `|` markers and `@{\vrule}` / `!{\vline}`-style hooks in multicolumn
specs also survive as row-scoped cell-level vertical rule metadata and emit
renderer-visible rule rectangles. Column-level vertical borders now also span
horizontal rule-only rows so table borders do not break across coarse `\hline`
or partial-rule display-list rows, while top/bottom rule-only rows trim those
vertical rectangles to the visible horizontal boundary. Partial horizontal rule
rows now suppress vertical-rule stubs outside the visible or trimmed partial
rule span, so coarse borders do not extend across whitespace-only rule gaps or
trimmed rule ends.
Simple visible `>{...}` / `<{...}` hooks and non-rule `@{...}` / `!{...}`
separator hooks in multicolumn specs now survive as cell-level prefix/suffix
metadata and decorate display-list cell text.
Simple `l` / `c` / `r` / paragraph-style table
preamble columns now survive as `TableColumnSpec` metadata, bounded `*{n}{...}`
repeated specs expand before IR construction, and those specs drive coarse
left/center/right padding in the display-list text fallback. Absolute
`p{...}` / `m{...}` / `b{...}` and array-package `w` / `W` column widths now
survive as point-sized hints and set minimum monospaced fallback column widths.
Simple table target-width expressions such as
`\dimexpr\textwidth-36pt\relax` and `\dimexpr\textwidth-2\tabcolsep\relax`
are also interpreted narrowly in the display-list fallback so `tabularx` /
`tabu` stretching can remain close to the requested target without promoting the
raw expression into visible text. If a target-width table has no flexible
paragraph-style column, the fallback stretches intercolumn separators instead of
adding trailing padding to the last column. Target-width stretching is capped to
the same line-character budget used by the fallback wrapper so common
`\textwidth` tables do not wrap solely because of character-count rounding.
Simple `@{...}` /
`!{...}` intercolumn visible separators now replace the default fallback
separator in display-list text, simple visible `>{...}` / `<{...}` cell hooks
now decorate display-list cell text, and common `>{\raggedleft}`,
`>{\centering}`, and `>{\raggedright}` alignment hooks now drive the following
column's coarse alignment while otherwise staying non-visible, and unknown
alphabetic custom column specs preserve column count as `Unknown` columns while
skipping bounded option/argument payloads. Simple `\newcolumntype` definitions
seen before a table can also drive coarse alignment/width metadata for matching
custom columns. Simple column border markers and simple `@{\vrule}` /
`@{\vline}` / `!{\vrule}` / `!{\vline}` array hooks also emit coarse vertical
`PageDisplayList::Rule` rectangles at the monospaced fallback boundary
positions, including repeated `||` rule-count approximations; top/bottom
horizontal-rule rows trim those vertical rule rectangles to the visible
horizontal rule boundary so borders no longer protrude above or below a boxed
table. When those borders are emitted as rule ops, the corresponding
display-list text separator is whitespace rather than a searchable `|` glyph.
`multirow` commands now preserve their visible cell text, but simple
`\multirow` / `\multirowcell` row counts also survive as
`TableCell.row_span` metadata. Multirow geometry is still not
modeled. Diagonal table header helpers such as `\diagbox`, `\slashbox`, and
`\backslashbox` normalize to readable `A/B` cell text without leaking helper
command names. Table-cell box wrappers such as `\rotatebox`, `\scalebox`,
`\resizebox`, and `\reflectbox` preserve visible body text while hiding
layout-only arguments, and overlap/height helpers such as `\rlap`, `\llap`,
`\clap`, and `\smash` preserve their visible body text. The first figure slice
is also implemented at
the renderer boundary: resolver-provided PNG/JPEG bytes can be decoded into PDF
`/Image` XObjects by `tex-pdf`, and project-root render-IR capture can now write
debug PDFs with those embedded assets. Image display-list boxes also honor the
common `\includegraphics` `width`, `height`/`totalheight`, and `scale` options
for common units and text/page-relative dimensions; option control sequences
such as `\textwidth` / `\linewidth` are preserved from the VM event through
display-list sizing instead of being normalized away as visible text. Common
page/content aliases such as `\paperwidth`, `\pagewidth`, `\hsize`, and
`\vsize` are accepted by the same dimension parser, and simple
`\dimexpr...\relax` addition/subtraction forms such as
`\dimexpr\textwidth-2\fboxsep\relax` are resolved for graphic size options.
Resolver-backed PNG/JPEG headers now provide natural pixel dimensions plus
optional density metadata, and resolver-backed SVG/PDF/EPS headers now provide
natural point dimensions for default aspect-preserving image boxes when no
explicit size is given. PNG `pHYs` and JPEG JFIF density fields are converted to
TeX points before layout uses the natural box; SVG `width`/`height` or
`viewBox`, PDF `/MediaBox`, and EPS
`%%BoundingBox`/`%%HiResBoundingBox` provide point-sized vector/document boxes.
Explicit `natwidth` / `natheight` graphic options can also override the
natural box when asset headers are unavailable or intentionally approximate.
When both `width` and `height` are present, `keepaspectratio` now fits the
natural/default image box inside the requested rectangle instead of stretching
it. `trim`, `viewport`/`bb`, and `clip` options are now preserved as
renderer-neutral `ImageCrop` metadata on display-list image
ops, exposed in SVG debug artifacts as `data-image-crop-*` attributes, and used
to derive default image-box size when no explicit size is provided; individual
`bbllx` / `bblly` / `bburx` / `bbury` keys are normalized to the same viewport
metadata. Whitespace-separated and braced comma-separated crop quads share the
same parser. The PDF bitmap embedder and display-list SVG debug renderer now
apply crop metadata by offsetting/scaling embedded bitmap assets; `clip=true`
additionally clips to the destination rect. Project-root render-IR artifacts
cover both clipped and unclipped crop placement for bitmap assets, including
starred `\includegraphics*` forms that imply `clip`. PDF `page` and `pagebox`
graphic options now survive as renderer-neutral page-selection metadata through
events, Document IR, and `PageDisplayList::Image`; external PDF conversion uses
the selected page and honors `cropbox`/other Ghostscript-supported page boxes
when converting debug artifacts, with the Poppler fallback honoring `cropbox`.
Local `draft`
graphic options,
package-level
`\usepackage[draft]{graphicx}` options, and class-level
`\documentclass[draft]{...}` global options forwarded through `graphicx` now
force renderer-visible placeholders even when the asset exists; preamble
`\PassOptionsToPackage{draft}{graphicx}` declarations are threaded through the
same path, as are `\setkeys{Gin}{...}` graphic defaults. These placeholders
preserve the image box without embedding the bitmap/vector asset. Missing
graphic assets now produce render-event diagnostics when the capture has enough
project or mounted-file context to know the asset is absent, while preserving
the image placeholder, and those diagnostics now
annotate `PageDisplayList::Image`, debug PDF placeholder text, and SVG debug
`data-image-*` attributes. Existing but unconvertible PDF/EPS assets surface as
unsupported-image placeholders, while resolved PDF/EPS assets can be converted
to PNG for debug display-list PDF/SVG artifacts through Ghostscript or Poppler.
Resolver-backed SVG and PNG/JPEG bitmap assets are embedded as data-URI
`<image>` elements in project-root display-list SVG debug artifacts, and simple
relative PNG/JPEG `href` / `xlink:href` references inside those SVG assets are
rewritten to data URIs through the same resolver. The SVG-internal image href
path accepts quote style and `=` whitespace variations, decodes XML attribute
entities, decodes URL percent escapes inside relative path components without
turning encoded slashes into separators, normalizes project-root-relative dot
components (`.` / `..`), strips query/fragment suffixes for resolver lookup, and
refuses fragment-only, absolute, raw or percent-decoded external-scheme/drive-like
first components, raw backslash/NUL, and root-escaping references.
Debug SVG output preserves existing `data:` and
fragment-only image refs, but sanitizes unresolved non-`data:` / non-fragment
image refs to inert `data:,` values rather than leaving browser-loadable URLs in
the generated artifact. Simple
resolver-backed SVG `<rect>`, `<line>`, `<circle>`, `<ellipse>`, `<polyline>`,
`<polygon>`, and `<path>` content, including percentage geometry for simple
rect/line/circle/ellipse attributes, with line, cubic/smooth cubic, and
quadratic/smooth quadratic commands plus endpoint-parameterized arcs and
multiple closed subpaths in one path element, including
    basic presentation/style fill and stroke metadata, simple `translate` /
    `scale` / `skewX` / `skewY` transform attributes, simple nested group transforms, inherited
root/group-level fill/stroke/absolute and percentage stroke-width presentation metadata, simple
root `preserveAspectRatio` viewport fitting for `none`, `meet`, and `slice`,
comment-tolerant `<style>` / CDATA type, class, id, element-qualified class/id,
and rightmost simple descendant/child selector approximation fill/stroke/stroke-width
rules with basic specificity, inline and style-rule `stroke-width: unset`
overriding class rules, and
    source-order cascade and same-property declaration order, `display: none` / `visibility: hidden` paint
    suppression with `display` and `visibility` `initial` / `unset` keyword
    handling, including inline/style-rule `display: inherit` and inline/style-rule `visibility: unset`
    overriding class rules and style-rule `inherit` / `unset` overriding presentation-attribute
    display/visibility,
    parse-tolerant `!important` value markers,
    3/4/6/8-digit hex/CSS/SVG named/`rgb(...)` /
    `rgba(...)` / `hsl(...)` / `hsla(...)` color forms, inherited
    `currentColor` fill/stroke paint, simple `inherit` / `initial` / `unset`
    paint/color values with inline/style-rule `unset` overriding class paint rules
    and style-rule `inherit` / `unset` overriding presentation-attribute paint/color,
    transparent paint as no-paint, simple gradient paint-server first-stop solid
    approximations with `href` inheritance, including alias `currentColor` stops
    without overriding stop-local color, inline/style-rule
    `stop-color` / `stop-opacity` cascade, `currentColor` stop colors with
    root/paint-server `color` CSS rules, and paint-server `url(...)` fallback colors,
    simple `fill-rule` mapped to PDF nonzero/even-odd fill
operators with `initial` reset handling, inline/style-rule `unset`
overriding class fill rules, and style-rule `inherit` / `unset` overriding
presentation-attribute fill rules, simple `opacity` / `fill-opacity` /
`stroke-opacity` mapped to PDF ExtGState resources with `initial` / `unset` reset
handling, inline/style-rule `opacity: inherit`, inline/style-rule `unset` overriding class fill/stroke opacity
rules, and style-rule `inherit` / `unset` overriding presentation-attribute opacity,
simple `stroke-dasharray` absolute/percentage lengths
mapped to PDF dash patterns with inline and style-rule `unset` overriding class
dash patterns and style-rule `inherit` / `unset` overriding presentation-attribute
dash patterns and offsets, `stroke-dashoffset` absolute/percentage phase support
with inline and style-rule `unset` overriding class offsets and style-rule
`inherit` / `unset` overriding presentation-attribute stroke widths/offsets, negative
phase normalization, and transform scaling, simple `stroke-linecap` /
`stroke-linejoin` / `stroke-miterlimit` mapped to PDF graphics state with
`initial` reset handling and inline/style-rule `unset` overriding class
line-style rules plus style-rule `inherit` / `unset` overriding
presentation-attribute line styles,
simple
`vector-effect: non-scaling-stroke` stroke-width/dash preservation with
`initial` / `unset` reset handling and inline/style-rule `inherit` handling,
including style-rule `inherit` overriding presentation-attribute vector
effects, and simple rect-backed `clipPath` clipping with
`initial` / `unset` reset handling and inline/style-rule `inherit` handling,
including style-rule `inherit` overriding presentation-attribute clip paths and
transformed group-wrapped rect children, and
    `matrix` / `rotate` / `skewX` / `skewY` transforms for path-like line/poly/path
primitives, plus non-axis-aligned transformed rectangles rendered as closed
vector polygons and transformed circle/ellipse primitives rendered as cubic
vector paths, including simple path-like children in `<defs>` group and
`symbol` definitions reused through `<use>`, including rounded `rect`
definitions, simple `<defs>` `<use>` aliases, and `<defs>` group children
composed from `<use>` aliases, plus symbol children composed from `<use>`
aliases and `<defs>` symbol aliases with basic symbol `viewBox`
viewport fitting, simple `<line>`, `<polyline>`, `<polygon>`, and `<path>`
arrow markers from path-like `<marker>` children, including cascaded
presentation/CSS `marker` shorthand and `marker-start` / `marker-mid` /
`marker-end` with `initial` reset handling and inline/style-rule `inherit` /
`unset` overriding class and presentation-attribute marker references,
`context-stroke` /
`context-fill` paint inheritance, rounded marker `rect` children, and nested group
transform/presentation state,
plus simple embedded `data:image/png` / `data:image/jpeg` SVG
`<image>` elements with `opacity` and `preserveAspectRatio` fitting, including
direct `id`-addressed `<defs><image>` reuse and simple image `<defs>` `<use>`
aliases through external `<use>`, plus simple group-contained image definitions
and group-contained image aliases reused through the group id, and simple
symbol-contained image definitions with basic symbol `viewBox` viewport fitting,
including simple symbol image aliases, is also
rendered directly as vector PDF drawing operations in
display-list PDF artifacts instead of falling back to unsupported-image
placeholders. Simple SVG vector PDF rendering also uses display-list
`ImageCrop` viewport/trim placement and `clip=true` destination clipping. The
display-list image op now carries the resolved natural point size separately
from the destination rectangle, so PDF/SVG debug crop placement uses the
original asset coordinate space even when a PDF/EPS asset is rendered through a
converted PNG/JPEG. The
internal compiler still writes legacy page PDFs for preview and also exports
revision-local `rev-N/render-ir/` event, IR, display-list, PDF, SVG, and
legacy-text debug artifacts for the real-rendering path; the revision artifact
route exposes the `render-ir/` JSON/TXT artifacts for inspection, and the
preview snapshot advertises those debug artifact URLs when the revision contains
them. `latexd serve --compiler-bin internal` now uses matching render-IR
display-list SVG pages as the default preview SVG path when the debug SVG page
count matches the internal compiler page count, naming those SVG artifacts from
filename-safe `PageDisplayList.page_id` rather than absolute page index while
retaining the legacy page PDF fallback artifacts. Production PDF/SVG vector
embedding remains deferred.
The `tex-pdf` display-list renderer now has an explicit converted-asset hook:
callers can resolve an original external asset and provide converted PNG/JPEG
bytes for PDF/EPS-style inputs without making the renderer depend directly on
Ghostscript or Poppler. Unconverted resolved PDF/EPS assets now surface as
unsupported placeholders instead of generic image placeholders.
`latexd` now wires that hook to Ghostscript CLI conversion for render-IR debug
artifacts, with a Poppler `pdftoppm` fallback for PDF assets, so resolved
PDF/EPS graphic assets can be converted to PNG for display-list PDF/SVG
artifacts when the relevant local tool is available. Converted debug artifacts
reuse the display-list natural point size for crop/clip placement rather than
the converted bitmap pixel size. Driver-accurate crop/clip rendering for
production SVG vector output and raster backends, broader driver-exact PDF crop
edge cases, TeX-exact rotated-box reflow, broader SVG style cascade beyond
root/group/simple selector
fill/stroke/stroke-width/opacity/marker specificity and color support, full SVG
compositing and broader stroke styling, programmable table
preamble hooks, exact residual vertical border trimming, exact table rule trimming, actual multirow
geometry, exact nested table layout/reflow, and full TeX alignment policy are
still deferred.
Rotation intent is no longer dropped: `angle` /
`origin` options and simple `\rotatebox` wrappers are preserved as
renderer-neutral `ImageRotation` metadata. The display-list PDF path applies
that metadata to embedded bitmap XObjects and unresolved-image placeholders,
and SVG debug artifacts apply the equivalent top-down transform.
Common graphic layout wrappers also feed the same option path: `resizebox`
contributes inherited width/height hints, `scalebox` contributes horizontal
scale plus optional vertical scale, `adjustbox` `xscale`/`yscale` affects
display-list image-box sizing, and `reflectbox` preserves reflection intent as
`xscale=-1` for later renderer work. `raisebox` and `parbox` wrappers also
preserve nested graphics without leaking layout dimensions into visible text.
Color box wrappers such as `\colorbox` and `\fcolorbox` likewise preserve
nested graphics while hiding color arguments, including optional color model
selectors, from extracted text.
`overpic` environments now preserve their backing image, graphic options, and
simple `\put` / `\multiput` text payloads as visible overlay approximations
without leaking overlay coordinates or package scaffolding into text; exact
overlay geometry is still deferred.
Legacy subfigure commands such as `\subfigure[...]{\includegraphics...}` share
the subfloat capture path, preserving panel images and captions without leaking
raw citation keys or layout arguments. Direct labels inside consumed
subfloat/subcaptionbox bodies are preserved as label-definition events instead
of being dropped with the wrapper body.
Two-optional caption forms such as `\subfloat[short][long]{...}` and legacy
`\subfigure[short][long]{...}` preserve the long visible caption while hiding
the short list caption. Starred `\subfloat*` / `\subfigure*` commands use the
same capture path without leaking the star marker.
Caption package boxes such as `\captionbox{...}{\includegraphics...}` share the
subcaptionbox capture path, preserving the image and caption while hiding width
options. Leading optional short/list captions such as
`\captionbox[short]{long}[width]{...}` and `\subcaptionbox[short]{long}{...}`
use the long visible caption and suppress the short caption from extracted
text. Starred forms such as `\captionbox*{...}{...}` and
`\subcaptionbox*{...}{...}` follow the same capture path without leaking the
star marker.
Caption package setup/list-entry helpers such as `\captionsetup`,
`\subcaptionsetup`, `\ContinuedFloat`, and `\captionlistentry` are consumed as
non-visible float metadata rather than emitted as body text.
Caption-like commands such as `\subcaption`, `\captionabove`, and
`\captionbelow` now enter the same caption event path as `\caption`, including
short-caption suppression and citation-key redaction, including when they occur
inside floatrow caption arguments.
Floatrow figure boxes such as
`\ffigbox[...][...]{\includegraphics...}{\caption{...}}` preserve the image and
caption while consuming floatrow layout options, including caption-first
argument order.
Floatrow side-caption boxes such as `\fcapside[...]{...}{...}` now scan both
braced arguments so the image and caption survive regardless of argument order.
Generic floatrow boxes such as
`\floatbox[\capbeside]{figure}[...][...]{...}{...}` likewise preserve nested
graphics and captions while consuming float type and layout options.
Floatrow table boxes such as `\ttabbox[...]{\begin{tabular}...\end{tabular}}`
also preserve the nested table and caption without leaking floatrow width
macros.
Legacy `epsf` sizing assignments such as `\epsfxsize=...` and
`\epsfysize=...` are converted into graphic width/height options for the next
`\epsfbox` / `\epsffile` image instead of leaking as text.
`picins` inline picture commands now preserve `\parpic` images and attach the
preceding `\piccaption` text without leaking placement or width hints into body
text.
`floatflt` `floatingfigure` / `floatingtable` environments now follow the same
float capture path as wrap/sidecap floats, preserving images, captions, and
labels without leaking position or width arguments into visible text.
`picinpar` `figwindow` / `tabwindow` environments now capture option-carried
objects and captions while preserving body labels and hiding window placement
arguments.
Tufte-style `marginfigure` / `margintable` environments now use the same
figure/table capture path, preserving images, captions, and labels without raw
environment fallback text.
`threeparttable` `measuredfigure` environments are likewise promoted to the
figure capture path so images and captions stay attached as a single graphic
block.
`rotfloat` now shares the rotating-package shim path, so sideways figures from
that package avoid missing-package diagnostics while retaining the same float
capture behavior.
Simple `fancybox` wrappers such as `\shadowbox`, `\ovalbox`, and `\doublebox`
now use the same graphic wrapper path as `\fbox`.
`psfrag` replacement helper commands are treated as layout/asset preprocessing
metadata around graphics, so replacement tags do not leak into body text.
`pstricks` `pspicture` environments now use bounded unsupported-picture
placeholders instead of rendering drawing command payloads as body text.
LaTeX `picture` environments use the same unsupported-picture placeholder path.
`PageDisplayList::Image` now carries optional `ImageScale` metadata, and SVG
debug artifacts expose that metadata as `data-image-scale-x` /
`data-image-scale-y`. Nested graphic wrappers now thread outer sizing and scale
hints into inner graphics instead of dropping them at the next wrapper boundary.
Common float alignment declarations such as `\centering`, `\RaggedRight`, and
`\justifying` are recognized as layout-only commands so they do not leak into
figure/table text or trigger missing-package diagnostics for `ragged2e`.

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
- report normalized unique overlap alongside raw overlap, currently folding
  Unicode Greek symbols to names, common ligatures, and soft hyphens;
- further normalize citation-number noise where it does not hide real text
  regressions;
- classify low metric causes in `metric_findings`, including build failure,
  low internal text count, low raw/normalized overlap,
  normalization-sensitive overlap, page-count drift, and first-page raster
  gross failure;
- split text metrics into body text, front matter, captions, references where
  the IR can identify them. The oracle now copies successful internal
  `document-ir.json` artifacts into the report directory and records
  `ir_structure_slices` for front matter, abstract, body, captions, tables,
  references, and fallback text with raw and normalized overlap against the
  official PDF text;
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
  `\email`, `\keywords`, and `\pacs`;
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

Implemented first slice:

- `GraphicRef` and `GraphicBlock` already preserve asset refs, formats, hashes,
  source spans, and captions into `PageDisplayList::Image`;
- `tex-pdf` can now embed resolver-provided PNG/JPEG assets as PDF image
  XObjects;
- project-root render-IR capture now resolves real source-root image files into
  the display-list PDF artifact path;
- project-root display-list SVG debug artifacts can embed resolver-provided SVG
  and PNG/JPEG bitmap assets as data-URI `<image>` elements, with clip-enabled
  crop metadata reflected in the debug SVG for bitmap assets, simple SVG
  assets with parseable natural dimensions, and converted PDF/EPS debug assets
  whose display-list ops carry resolved natural point dimensions;
- display-list PDF artifacts can render simple resolver-provided SVG `<rect>`,
  `<line>`, `<circle>`, `<ellipse>`, `<polyline>`, `<polygon>`, and `<path>`
  content, including percentage geometry for simple rect/line/circle/ellipse
  attributes, with line, cubic/smooth cubic, quadratic/smooth quadratic, and arc
  commands, including multiple closed subpaths in one path element, basic
  presentation/style fill and stroke metadata plus
  simple `translate` / `scale` / `skewX` / `skewY` transforms, simple nested group transforms, and
  inherited root/group-level fill/stroke/absolute and percentage stroke-width
  metadata, simple root
  `preserveAspectRatio` viewport fitting for `none`, `meet`, and `slice`, simple
  comment-tolerant `<style>` / CDATA universal, type, class, id, element-qualified
  class/id, and rightmost simple descendant/child selector approximation
  fill/stroke/stroke-width rules with basic specificity, inline and style-rule
  `stroke-width: unset` overriding class rules, source-order cascade, and same-property declaration order
  plus `display: none` / `visibility: hidden` paint suppression with
  `display` and `visibility` `initial` / `unset` keyword handling and
  inline/style-rule `display: inherit` and inline/style-rule `visibility: unset` overriding class rules
  and style-rule `inherit` / `unset` overriding presentation-attribute display/visibility,
  plus parse-tolerant `!important` value markers and 3/4/6/8-digit
  hex/CSS/SVG named/`rgb(...)` /
  `rgba(...)` / `hsl(...)` / `hsla(...)` color forms, inherited
  `currentColor` fill/stroke paint, simple `inherit` / `initial` / `unset`
  paint/color values with inline/style-rule `unset` overriding class paint rules
  and style-rule `inherit` / `unset` overriding presentation-attribute paint/color,
  transparent paint as no-paint, simple gradient paint-server first-stop solid
  approximations with `href` inheritance, including alias `currentColor` stops
  without overriding stop-local color, inline/style-rule
  `stop-color` / `stop-opacity` cascade, `currentColor` stop colors with
  root/paint-server `color` CSS rules, and paint-server `url(...)` fallback colors,
  simple `fill-rule` mapped to PDF
  nonzero/even-odd fill operators with `initial` reset handling,
  inline/style-rule `unset` overriding class fill rules, and style-rule
  `inherit` / `unset` overriding presentation-attribute fill rules, simple
  `opacity` / `fill-opacity` /
  `stroke-opacity` mapped to PDF ExtGState resources with `initial` / `unset` reset
  handling, inline/style-rule `opacity: inherit`, inline/style-rule `unset` overriding class fill/stroke opacity
  rules, and style-rule `inherit` / `unset` overriding presentation-attribute opacity,
  simple `stroke-dasharray`
  absolute/percentage lengths mapped to PDF dash patterns with inline and style-rule
  `unset` overriding class dash patterns and style-rule `inherit` / `unset`
  overriding presentation-attribute dash patterns and offsets, `stroke-dashoffset`
  absolute/percentage phase support with inline and style-rule `unset` overriding
  class offsets and style-rule `inherit` / `unset` overriding presentation-attribute
  stroke widths/offsets, negative phase normalization, and transform
  scaling, simple
  zero `stroke-width` suppression, `stroke-linecap` / `stroke-linejoin` /
  `stroke-miterlimit` mapped to PDF graphics state with `initial` reset
  handling and inline/style-rule `unset` overriding class line-style rules plus
  style-rule `inherit` / `unset` overriding presentation-attribute line styles, simple
  `vector-effect: non-scaling-stroke` stroke-width/dash preservation with
  `initial` / `unset` reset handling and inline/style-rule `inherit` handling,
  including style-rule `inherit` overriding presentation-attribute vector
  effects, simple rect-backed `clipPath` clipping with
  `initial` / `unset` reset handling and inline/style-rule `inherit` handling,
  including style-rule `inherit` overriding presentation-attribute clip paths and
  transformed group-wrapped rect children, path-like `matrix` / `rotate` / `skewX` /
  `skewY` transforms, non-axis-aligned
  transformed rectangles, and transformed circle/ellipse cubic paths, as vector
  drawing operations, including simple path-like children in `<defs>` group and
  `symbol` definitions reused through `<use>`, including rounded `rect`
  definitions, simple `<defs>` `<use>` aliases, and `<defs>` group children
  composed from `<use>` aliases, plus symbol children composed from `<use>`
  aliases and `<defs>` symbol aliases with basic symbol `viewBox`
  viewport fitting, simple `<line>`, `<polyline>`, `<polygon>`, and `<path>`
  arrow markers from path-like `<marker>` children, including cascaded
  presentation/CSS `marker` shorthand and `marker-start` / `marker-mid` /
  `marker-end` with `initial` reset handling and inline/style-rule `inherit` /
  `unset` overriding class and presentation-attribute marker references,
  `context-stroke` / `context-fill` paint inheritance, rounded marker `rect`
  children, and nested group transform/presentation state, plus simple embedded
  `data:image/png` / `data:image/jpeg`
  SVG `<image>` elements as PDF XObjects with `opacity` and `preserveAspectRatio`
  fitting, including direct `id`-addressed `<defs><image>` reuse and simple
  image `<defs>` `<use>` aliases through external `<use>`, simple
  group-contained image definitions and group-contained image aliases reused
  through the group id, simple symbol-contained image definitions with basic symbol `viewBox` viewport
  fitting, including simple symbol image aliases, display-list crop/viewport
  placement, and
  `clip=true` destination clipping for simple SVG vector PDF assets;
- default regression coverage exercises both PNG and JPEG bitmap embedding in
  display-list PDF and debug SVG artifacts;
- missing or undecodable assets still render as bounded placeholders in both
  display-list PDF and debug SVG artifacts instead of deleting figure space or
  captions.

Remaining figure work:

- broader option-aware sizing and driver-exact bounding-box behavior;
- fuller wrapper sizing semantics for nested boxes and TeX-exact wrapper
  reflow;
- trim/viewport/clip rendering parity for production SVG vector output, raster
  backends, and broader driver-exact PDF edge cases;
- TeX-exact rotated-box dimensions, page reflow, and non-debug raster parity;
- external PDF/EPS conversion and production SVG/PDF vector embedding or raster
  insertion;
- raster tests that fail on missing major figure regions.

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
- support `&`, `\\`, `\tabularnewline`, `\hline`, `\cline`, `\toprule`,
  `\midrule`, `\bottomrule`, and related booktabs rule commands;
- support basic `l`, `c`, `r`, and paragraph columns;
- handle `\multicolumn` and `\multirow` as approximations;
- render table caption and label;
- fall back to monospace text table when structure is too complex.

Done when:

- tables are readable;
- table content is no longer flattened into broken inline text;
- table captions and labels remain discoverable.

Implemented first slice:

- `tabular`, `tabular*`, `tabularx`, `array`, `longtable`, `tabu`, and
  `longtabu` emit bounded table-fallback events instead of raw body text;
- `array` environment fallback now has dedicated VM and integration coverage for
  column metadata, partial rules, and display-list rule ops;
- `DocumentIrBuilder` promotes those events into `Table` IR with rows, cells,
  caption, and label-preserving source provenance;
- the text-only display-list path renders table caption and rows with a
  monospaced font request;
- full and partial horizontal rules now also emit `PageDisplayList::Rule`
  rectangles for renderer-visible table separators, and partial-rule gaps stay
  whitespace rather than visible filler text. Horizontal rule rows no longer
  emit separate `TextRun` operations, keeping rule strokes out of searchable PDF
  text. Partial horizontal rule rectangles use the same visible separator
  widths as table row text instead of assuming fixed-width default separators,
  and simple `booktabs` `\cmidrule[...](l/r/lr){...}` optional rule widths and
  trim options, including `l{...}` / `r{...}` trim length payloads, shorten the
  requested side of those partial rule rectangles in point units without letting
  rule-width payloads or trim unit names affect visible table text.
- display-list padding for `\multicolumn` spanning cells uses the visible
  separator widths from the covered columns, instead of assuming fixed-width
  default separators.
- simple `l` / `c` / `r` / paragraph-style table preamble columns and bounded
  `*{n}{...}` repeated specs now survive into IR and drive coarse display-list
  text alignment.
- `tabularx` `X` columns and array-package `w{align}{width}` /
  `W{align}{width}` columns are treated as paragraph/aligned fixed-width
  fallback columns; absolute `p` / `m` / `b` / `w` / `W` widths now survive as
  point-sized hints and drive coarse minimum display-list fallback widths.
- `tabular*` / `tabularx` target-width arguments and simple `tabu` /
  `longtabu` `to` / `spread` width specs now survive as table-level width
  metadata; the readable display-list fallback resolves common specs such as
  `\textwidth` / `\linewidth`, plus simple `\dimexpr` addition/subtraction
  forms, and stretches flexible paragraph-style columns toward that target
  width. If no flexible column is available, the fallback stretches separators
  rather than padding the final column. Target-width stretching is capped to the
  fallback line-character budget so `\textwidth` tables do not wrap solely
  because of coarse character rounding.
- array-package hook and intercolumn modifiers `>{...}`, `<{...}`, `@{...}`,
  and `!{...}` are skipped as non-column material so the following real columns
  still drive fallback alignment. Simple visible `@{...}` / `!{...}`
  intercolumn material now survives as display-list separator text, while
  simple visible `>{...}` / `<{...}` cell material now decorates display-list
  cell text; common `>{\raggedleft}`, `>{\centering}`, and
  `>{\raggedright}` hooks preserve coarse alignment intent; simple
  `@{\vrule}` / `@{\vline}` / `!{\vrule}` / `!{\vline}` hooks preserve coarse
  vertical rule metadata; non-visible spacing hooks such as
  `@{\extracolsep{\fill}}` are suppressed instead of becoming display-list
  separators; style-only declaration hooks such as `>{\bfseries\itshape}` and
  `<{\normalfont}` and programmable `collectcell` hooks are ignored without
  leaking command names into table text.
- unknown alphabetic custom column specs such as `L{...}` / `Y[...]` preserve
  column count as `Unknown` columns and skip their bounded option/argument
  payloads instead of parsing those payloads as extra columns.
- simple pre-table `\newcolumntype` definitions such as
  `\newcolumntype{L}[1]{>{...}p{#1}}` are interpreted narrowly enough to
  preserve replacement-derived alignment and width metadata; first-argument
  defaults from definitions such as `\newcolumntype{Q}[2][c]{...}` are now
  applied inside normal and repeated table preambles.
- simple `tabu`/`longtabu` preambles, including `longtabu to ... {cols}` and
  `X[...]` options, are normalized into the same table column metadata.
- common numeric `siunitx` `S[...]` and `dcolumn` `D{...}{...}{...}` columns
  are treated as decimal-aligned fallback columns with coarse separator alignment.
- simple vertical border markers and simple `@{\vrule}` / `@{\vline}` /
  `!{\vrule}` / `!{\vline}` array hooks now emit coarse
  `PageDisplayList::Rule` rectangles at the readable table fallback's column
  boundaries, including repeated `||` rule-count approximations; when those
  borders are rendered as rule ops, the corresponding display-list text
  separator is whitespace rather than a searchable `|` glyph. Adjacent
  same-position vertical rule rects from consecutive table and horizontal-rule
  rows are merged into longer display-list rule ops, so coarse borders stay
  continuous across `\hline`, `\cline`, and `\cmidrule` rows. Partial
  horizontal rule rows filter those vertical-rule stubs to the visible and
  trim-adjusted dash span instead of extending them across whitespace-only gaps
  or trimmed rule ends.
- common `booktabs` spacing and rule-control commands such as optional-width
  `\toprule` / `\midrule` / `\bottomrule`, `\addlinespace`,
  `\morecmidrules`, and `\specialrule` are suppressed from visible table text
  while preserving renderer-visible rule metadata where appropriate; table
  `\noalign{...}` spacing bodies are also consumed without becoming visible
  fallback text.
- common `hhline` rule commands are suppressed from visible table text and
  treated as coarse full-width table rules, with simple `-` / `=` / `#` rule
  columns, `~` blank columns, `>{...}` modifiers, and bounded `*{n}{...}`
  repeated patterns preserved as partial rule spans without leaking pattern
  payloads; exact pattern semantics are deferred.
- common `arydshln` dashed rule commands such as `\hdashline`,
  `\firsthdashline`, `\lasthdashline`, and `\cdashline{...}` are suppressed
  from visible table text and mapped to coarse full/partial rule metadata.
- common `colortbl` / `xcolor` table color commands such as `\rowcolor`,
  `\rowcolors`, `\hiderowcolors`, `\showrowcolors`, `\cellcolor`,
  `\columncolor`, `\arrayrulecolor`, `\definecolor`, `\providecolor`, and
  `\colorlet` are suppressed from visible table text, including color-only
  column and multicolumn hooks; color styling is not rendered yet.
- simple `\multicolumn` alignment specs survive as `TableCell.alignment` and
  drive readable display-list padding for spanning cells.
- simple `|` markers and `@{\vrule}` / `!{\vline}` hooks in `\multicolumn`
  specs survive as cell-level vertical rule counts and emit row-scoped
  display-list rule rectangles.
- simple visible `>{...}` / `<{...}` hooks and non-rule `@{...}` / `!{...}`
  separator hooks in `\multicolumn` specs survive as cell-level display-list
  prefix/suffix metadata; style-only and non-visible spacing hooks in
  `\multicolumn` specs are ignored without leaking command names or spacing
  payloads into table text.
- common font declaration commands such as `\small`, `\scriptsize`,
  `\footnotesize`, `\fontsize{...}{...}`, and `\selectfont` are suppressed from
  body and table fallback text; font styling is not rendered yet.
- `multirow` / `multirowcell` commands now preserve visible cell text and simple
  `row_span` metadata in the table fallback.
- common `multirow` positioning, bigstrut, and vertical-move optional arguments
  are consumed without leaking layout hints into visible fallback text.
- continuation rows below a simple multirow cell now insert a blank placeholder
  column when the spanned column is omitted, so following cells are placed under
  the next table column in the readable fallback.
- negative `\multirow{-n}` counts are treated as upward-span approximations and
  do not create downward continuation placeholders in later rows.
- `\multirow{...}{...}{\multicolumn{...}{...}{...}}` and
  `\multirowcell{...}{\multicolumn{...}{...}{...}}` now preserve the nested
  multicolumn's simple span/alignment/rule metadata so continuation-row
  placeholders cover the approximated spanned columns.
- starred `makecell` helpers such as `\makecell*{...}` and `\thead*{...}` are
  normalized to visible cell text without leaking helper command names.
- `makecell` / `thead` / `shortstack` internal line breaks are consumed inside
  the current cell and rendered as spaces in the readable fallback.
- rotated `makecell` helpers such as `\rotcell[...]{...}` and `\rothead{...}`
  preserve visible cell text without leaking rotation options.
- `makecell` gaped-cell helpers such as `\Gape`, `\gape`,
  `\makegapedcells`, and `\setcellgapes` preserve visible cell text without
  leaking gape dimensions or helper names.
- `pbox` table-cell helpers preserve visible body text and internal line breaks
  without leaking width or vertical-position arguments.
- `makecell` rule helpers `\Xhline{...}` and `\Xcline{...}{...}` are
  normalized to full/partial table rule metadata without leaking rule widths.
- diagonal table header helpers `\diagbox`, `\slashbox`, and `\backslashbox`
  normalize to readable two-label cell text.
- table-cell box wrappers such as `\rotatebox`, `\scalebox`, `\resizebox`, and
  `\reflectbox` normalize to visible body text without leaking layout arguments.
- table-cell overlap/height helpers such as `\rlap`, `\llap`, `\clap`, and
  `\smash` normalize to their visible body text.
- table-cell phantom helpers such as `\phantom`, `\hphantom`, and `\vphantom`
  hide their invisible payloads instead of leaking them into fallback text.
- table-cell spacing and visual-rule helpers such as `\strut`, `\bigstrut`,
  `\rule`, `\hrulefill`, and `\dotfill` are suppressed from fallback text so
  layout dimensions do not render as cell content.
- table-local layout spacing helpers such as `\hspace{...}`, `\vspace{...}`,
  `\pagebreak[...]`, `\hfill`, and `\smallskip` are consumed without leaking
  spacing arguments into table text.
- common table layout defaults such as `\arraystretch`, `\tabcolsep`,
  `\arrayrulewidth`, and `\extrarowheight` are defined so local
  `\renewcommand` / `\setlength` tweaks do not leak or produce undefined
  control-sequence diagnostics.
- table environments inside box wrappers such as `\resizebox{...}{...}{...}`
  are still captured as table IR instead of being swallowed by wrapper handling.
- `adjustbox` environments hide their option argument and allow nested tables to
  use the normal table fallback/IR path.
- `threeparttable` note markers such as `\tnote{a}` normalize to readable
  bracketed markers while preserving `tablenotes` bodies without leaking note
  macro syntax or note layout options.
- `tablefootnote` notes inside table cells preserve readable note text and
  citation placeholders without leaking footnote command syntax into cell text.
- simple table environments inside a table cell are flattened into readable cell
  text while keeping the outer table rows intact and hiding the nested table
  preamble/control text.
- common `longtable` repeated head/foot template delimiters
  `\endfirsthead`, `\endhead`, `\endfoot`, and `\endlastfoot` are consumed
  without leaking delimiter names or repeated header/footer template text into
  table fallback output.

Remaining table work:

- exact column width policy, programmable column hooks, and fuller multirow
  geometry rendering approximations;
- exact nested table layout/reflow beyond readable flattened cell text;
- residual vertical-border and exact rule-trimming edge cases in
  `PageDisplayList`;
- stronger booktabs/array-package compatibility on corpus fixtures;
- broader raster-oriented table readability gates on corpus fixtures.

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

Current status:

- inline/display math events preserve raw TeX source and carry optional
  readable `normalized_text`;
- the first normalized-text subset covers common Greek names, comparison and
  arithmetic operators, relation/equivalence/order operators including split
  `\not` negation forms, set/logical operators, fractions, binomial coefficients,
  common binary operators such as `oplus`,
  `otimes`, and `odot`,
  roots,
  text/operator wrappers, simple
  superscript/subscript braces, large-operator scripts including `substack` and
  `bigcup` / `bigcap` / `bigoplus`-style operators,
  style and limit controls such as `displaystyle`, `limits`, and `nolimits`,
  stack relation wrappers such as `overset`, `underset`, and `stackrel`, named
  operators and symbols such as `ell`, `aleph`, `hbar`, `Re`, `Im`, `prime`,
  `dagger`, `bigcirc`, and `backslash`, common
  extended arrows such as `xrightarrow` and `xleftarrow`, and arrow variants
  such as `Longrightarrow`, `leftrightarrow`, and `hookrightarrow`,
  common math alphabet wrappers, accent wrappers, brace grouping wrappers,
  delimiter commands including invisible `left.` / `right.` delimiters,
  `middle` / `bigm` delimiter controls, `\|` double-bar delimiters, and `lceil`
  / `lfloor` pairs, punctuation/remainder symbols such as `colon`, `mod`,
  `gets`, `nleftrightarrow`, and `triangleright`, ellipsis commands such as
  `ldots`, `cdots`, and `dots`, matrix/cases/array-style
  environments, alignment markers,
  and multiline row separators, plus nested amsmath environments such as
  `split`, `gathered`, and `alignedat`;
- document IR and page display lists already prefer normalized math text when
  present while keeping raw source available as fallback.
- unsupported math commands intentionally leave `normalized_text` empty so raw
  source remains visible instead of producing lossy ASCII stubs.

Remaining math work:

- richer math grouping and nested environment fidelity remain subset work;
- renderer-level glyph shaping and true math layout are intentionally deferred;
- corpus metrics still need math-heavy fixture gates once the subset expands.

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
  structure through title-block metadata events rather than body-text leakage;
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
- add raw and normalized text metrics.

Exit criteria:

- every CC0 smoke case reports page count, raw ratio, normalized ratio, and
  raster gross status;
- low ratio and gross page/raster causes are classified.

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

Status:

- table IR and basic monospaced display-list rendering are started;
- `tabularx` environments are promoted through the same table IR/display-list
  fallback path, with `X` columns mapped to paragraph-style columns and simple
  `\dimexpr` target widths resolved during display-list stretching;
- simple `tabu` and `longtabu` environments are promoted through the same
  table IR/display-list fallback path;
- array-package `w` / `W` fixed-width columns preserve coarse alignment intent;
- array-package hook/intercolumn modifiers are ignored as non-column material
  while preserving the following real columns, and common alignment hooks
  preserve coarse column alignment intent;
- unknown custom column specs preserve coarse column count as `Unknown`;
- simple `\newcolumntype` replacements can drive coarse custom-column alignment
  and width metadata;
- simple `\multicolumn` specs preserve coarse cell-level alignment intent;
- simple `\multicolumn` vertical rule specs and vrule/vline hooks emit
  row-scoped display-list rule ops;
- simple `\multicolumn` visible cell and separator hooks decorate display-list
  cell text;
- diagonal table header helpers normalize to readable cell text;
- table-cell box wrappers hide layout arguments while preserving visible text;
- table-cell overlap wrappers preserve visible text;
- `siunitx` `S` and `dcolumn` `D` numeric columns preserve coarse decimal
  alignment intent;
- figure asset identity/caption propagation exists, and resolver-provided
  PNG/JPEG bytes can be embedded by `tex-pdf`;
- project-root render-IR debug capture can feed real image files into the
  display-list PDF artifact;
- project-root display-list SVG debug artifacts can embed resolver-backed SVG
  and PNG/JPEG bitmap assets, including clip-enabled crop visualization for
  bitmap assets, simple SVG assets with parseable natural dimensions, and
  simple relative SVG-internal PNG/JPEG `href` / `xlink:href` assets rewritten
  to data URIs with quote/whitespace-tolerant attribute parsing, XML attribute
  entity decoding, URL percent decoding inside relative path components,
  parent-relative normalization, query/fragment stripping for lookup, raw
  backslash or ASCII-control href rejection plus raw or percent-decoded
  scheme/drive-like first-component rejection after XML entity decoding and
  before resolver lookup, percent-encoded slash, backslash, and ASCII control
  bytes preserved instead of becoming path characters, and unresolved
  non-`data:`/non-fragment references sanitized to inert `data:,` values instead
  of remaining browser-loadable URLs;
- project-root display-list PDF debug artifacts render simple resolver-backed
  SVG rectangle, rounded rectangle, line, circle, ellipse, polyline, polygon,
  percentage geometry for simple rect/line/circle/ellipse attributes,
  direct text content with simple XML named and numeric text entity decoding,
  simple `xml:space="preserve"` edge whitespace and whitespace-only `tspan`
  gaps/bodies, simple `text-anchor` with `initial` reset handling and
  inline/style-rule `unset` overriding class anchor rules plus style-rule
  `inherit` / `unset` overriding presentation-attribute anchors, numeric
  `dx`/`dy`, absolute and
  percentage `font-size`, `font-family`, `font-weight`, and `font-style`
  cascade with `initial` reset handling and inline/style-rule `unset`
  overriding class font rules plus style-rule `inherit` / `unset` overriding
  presentation-attribute font properties, simple `letter-spacing` and
  `word-spacing` via PDF text state with `initial` reset handling and
  inline/style-rule `unset` overriding class spacing rules plus style-rule
  `inherit` / `unset` overriding presentation-attribute spacing,
  simple `textLength` spacing adjustment via PDF character spacing, simple PDF
  base-family mapping, simple stroked text via PDF text rendering modes, simple
  `paint-order: stroke fill` ordering for stroked text and filled/stroked
  shapes with `initial` reset handling, inline/style-rule `unset` overriding class order rules,
  and style-rule `inherit` / `unset` overriding presentation-attribute order, simple fill/stroke opacity separation
  and dash/style state for stroked text, simple
  transformed text matrices, simple transformed `text-decoration` lines with
  `text-decoration-color`/`text-decoration-thickness`, dashed/dotted/double/wavy
  `text-decoration-style`, shorthand color/thickness/style, and `initial` /
  `unset` reset handling for decoration line/color/thickness/style plus
  inline/style-rule `inherit` handling for decoration line/color/thickness/style state,
  including style-rule `inherit` overriding presentation-attribute decoration state, simple
  `clip-path` rectangles with `initial` / `unset` reset handling and
  inline/style-rule `inherit` handling, including style-rule `inherit`
  overriding presentation-attribute clip paths, approximate
  `middle`/`central` baseline alignment and simple `baseline-shift` with
  `initial` reset handling, inline/style-rule `unset` overriding class baseline
  rules, and style-rule `inherit` / `unset` overriding presentation-attribute
  baseline properties,
  and multiple
  direct literal `tspan` children with numeric `dx`/`dy`, simple `tspan`
  leading/trailing text spaces, mixed literal text plus simple `tspan`
  children, and the same simple baseline alignment/shift behavior, and path
  content with line,
  cubic/smooth cubic,
  quadratic/smooth quadratic, and arc commands,
  including multiple closed subpaths in one path element and simple `<defs>`
  path-like reuse for `path`, simple `rect` including rounded corners, `circle`, `ellipse`,
  `line`, `polyline`, and `polygon` definitions, plus simple `<g>` and
  `symbol` definitions containing those path-like children, including symbol
  `viewBox` to `<use>` viewport fitting, simple `<defs>` `<use>` aliases,
  simple `<defs>` group children composed from `<use>` aliases, and simple
  symbol children composed from `<use>` aliases, simple literal `<text>`
  definitions, simple `tspan` children, text-use aliases, group-contained
  text, group-contained text-use aliases, symbol-contained text,
  symbol-contained text-use aliases, and symbol text aliases with symbol
  `viewBox` fitting reused through `<use>`, plus simple `<defs>` symbol aliases
  with symbol `viewBox` fitting,
  through whitespace-tolerant `href` / `xlink:href` `<use>`
  references, basic
  presentation/style fill and stroke metadata plus simple
  `translate` / `scale` / `skewX` / `skewY` transform attributes, simple nested group transforms,
  inherited root/group-level fill/stroke/absolute and percentage stroke-width
  metadata, simple root
  `preserveAspectRatio` viewport fitting for `none`, `meet`, and `slice`, simple
  comment-tolerant `<style>` / CDATA universal, type, class, id, element-qualified
  class/id, and rightmost simple descendant/child selector approximation
  fill/stroke/stroke-width rules with basic specificity, inline and style-rule
  `stroke-width: unset` overriding class rules, source-order cascade, and same-property declaration order
  plus `display: none` / `visibility: hidden` paint suppression with
  `display` and `visibility` `initial` / `unset` keyword handling and
  inline/style-rule `display: inherit` and inline/style-rule `visibility: unset` overriding class rules
  and style-rule `inherit` / `unset` overriding presentation-attribute display/visibility,
  plus parse-tolerant `!important` value markers and 3/4/6/8-digit
  hex/CSS/SVG named/`rgb(...)` /
  `rgba(...)` / `hsl(...)` / `hsla(...)` color forms, inherited
  `currentColor` fill/stroke paint, simple `inherit` / `initial` / `unset`
  paint/color values with inline/style-rule `unset` overriding class paint rules
  and style-rule `inherit` / `unset` overriding presentation-attribute paint/color,
  transparent paint as no-paint, simple gradient paint-server first-stop solid
  approximations with `href` inheritance, including alias `currentColor` stops
  without overriding stop-local color, inline/style-rule
  `stop-color` / `stop-opacity` cascade, `currentColor` stop colors with
  root/paint-server `color` CSS rules, and paint-server `url(...)` fallback colors,
  simple `fill-rule` mapped to PDF
  nonzero/even-odd fill operators with `initial` reset handling,
  inline/style-rule `unset` overriding class fill rules, and style-rule
  `inherit` / `unset` overriding presentation-attribute fill rules, simple
  `opacity` / `fill-opacity` /
  `stroke-opacity` mapped to PDF ExtGState resources with `initial` / `unset` reset
  handling, inline/style-rule `opacity: inherit`, inline/style-rule `unset` overriding class fill/stroke opacity
  rules, and style-rule `inherit` / `unset` overriding presentation-attribute opacity,
  simple `stroke-dasharray`
  absolute/percentage lengths mapped to PDF dash patterns with inline and style-rule
  `unset` overriding class dash patterns and style-rule `inherit` / `unset`
  overriding presentation-attribute dash patterns and offsets, `stroke-dashoffset`
  absolute/percentage phase support with inline and style-rule `unset` overriding
  class offsets and style-rule `inherit` / `unset` overriding presentation-attribute
  stroke widths/offsets, negative phase normalization, and transform
  scaling, simple
  zero `stroke-width` suppression, `stroke-linecap` / `stroke-linejoin` /
  `stroke-miterlimit` mapped to PDF graphics state with `initial` reset
  handling and inline/style-rule `unset` overriding class line-style rules plus
  style-rule `inherit` / `unset` overriding presentation-attribute line styles, simple
  `vector-effect: non-scaling-stroke` stroke-width/dash preservation with
  `initial` / `unset` reset handling and inline/style-rule `inherit` handling,
  including style-rule `inherit` overriding presentation-attribute vector
  effects, simple rect-backed `clipPath` clipping with
  `initial` / `unset` reset handling and inline/style-rule `inherit` handling,
  including style-rule `inherit` overriding presentation-attribute clip paths and
  transformed group-wrapped rect children, and path-like `matrix` / `rotate` / `skewX` /
  `skewY` transform attributes plus
  non-axis-aligned transformed rectangles and transformed circle/ellipse cubic
  paths, with root/style/element scanners requiring tag-name boundaries to
  avoid prefix false positives such as `svgz` as `svg`, `stylesheet` as
  `style`, or `linearGradient` as `line`, plus simple embedded
  `data:image/png` / `data:image/jpeg` and resolver-backed relative PNG/JPEG
  SVG `<image>` elements as PDF XObjects with `opacity` and
  `preserveAspectRatio` fitting,
  as vector PDF drawing operations,
  including display-list crop/viewport placement and `clip=true` destination
  clipping for simple SVG vector PDF assets;
- `latexd render-ir --root ... --input ... --output-dir ...` exposes the
  event/IR/display-list artifact pipeline without replacing the serve preview
  path;
- internal compiler revisions also write `rev-N/render-ir/` debug artifacts so
  the event/IR/display-list pipeline can be inspected next to existing page
  artifacts;
- the revision artifact route exposes `render-ir/` JSON/TXT artifacts while
  keeping non-render-IR metadata files private;
- preview snapshots advertise `render_ir_artifacts` URLs when an internal
  compiler revision has written the debug artifact bundle;
- `latexd serve --compiler-bin internal` now uses render-IR display-list SVG page
  images by default when the debug SVG page count matches the internal compiler
  page count, deriving those SVG URLs from filename-safe `PageDisplayList.page_id`
  values in `page-display-list.json` while keeping legacy page PDFs as fallback
  artifacts;
- `tex-pdf` can now consume caller-supplied converted PNG/JPEG bytes for
  resolved PDF/EPS-style display-list image assets, which keeps the renderer
  independent from the eventual Ghostscript/Poppler conversion layer;
- `latexd` now uses a Ghostscript CLI converter to supply PNG bytes for resolved
  PDF/EPS render-IR graphic assets in debug display-list PDF/SVG artifacts when
  `gs` is installed, and falls back to Poppler `pdftoppm` for PDF assets when
  Ghostscript is unavailable or cannot convert the asset;
- converted PDF/EPS debug assets use the original display-list natural point
  size for crop/clip placement instead of the converted bitmap pixel size;
- resolved but unconverted PDF/EPS assets surface as unsupported placeholders in
  display-list PDF/SVG output instead of falling back to generic image labels;
- `\includegraphics` option control sequences such as `\textwidth` /
  `\linewidth` survive event capture into display-list sizing, including
  simple `\dimexpr...\relax` addition/subtraction forms for graphic size
  options;
- `\paperwidth`, `\pagewidth`, `\hsize`, and `\vsize` are accepted as graphic
  dimension aliases;
- bitmap and simple SVG/PDF/EPS natural-size layout is available;
- explicit `natwidth` / `natheight` graphic options can drive default
  image-box sizing;
- individual `bbllx` / `bblly` / `bburx` / `bbury` options are normalized to
  viewport metadata for default image-box sizing;
- legacy `graphics` two-optional bounding-box syntax
  `\includegraphics[llx,lly][urx,ury]{...}` is normalized to viewport metadata
  instead of dropping the image;
- PDF `page` / `pagebox` graphic options survive into `GraphicRef`,
  `GraphicBlock`, and `PageDisplayList::Image`, and debug PDF/SVG asset
  conversion uses the selected page and supported PDF page boxes when
  available;
- local, package-level, class-level, `PassOptionsToPackage`, and
  `\setkeys{Gin}{...}` `draft` graphic options force placeholders instead of
  embedding resolver-backed assets;
- non-uniform graphic scale hints affect display-list image-box sizing;
- optional `ImageScale` metadata reaches `PageDisplayList::Image` and SVG
  debug artifacts;
- nested graphic wrappers preserve inherited sizing and scale hints;
- color-box graphic wrappers preserve nested images without leaking color
  arguments or optional color model selectors into IR or display-list text;
- `overpic` environments preserve backing images, graphic options, and simple
  `\put` / `\multiput` text payloads without emitting overlay coordinate
  commands as text;
- legacy `\subfigure` commands preserve nested panel images and captions through
  the same event/IR/display-list path as `\subfloat`;
- direct labels inside subfloat/subcaptionbox bodies survive as label
  definitions;
- two-optional `\subfloat` / `\subfigure` commands preserve the long visible
  caption without leaking the short list caption;
- starred `\subfloat` / `\subfigure` commands preserve nested panel images and
  captions without leaking the star marker;
- `\captionbox` commands preserve nested images and captions through the same
  path as `\subcaptionbox`, including leading optional short/list captions and
  starred forms;
- caption package setup/list-entry helpers are suppressed without leaking
  option or entry payloads;
- `\subcaption`, `\captionabove`, and `\captionbelow` are captured as captions
  without leaking short captions or raw citation keys, including inside
  floatrow boxes;
- floatrow `\ffigbox` commands preserve nested images and captions while
  suppressing floatrow layout options, including caption-first argument order;
- floatrow `\fcapside` commands preserve nested images and captions while
  suppressing side-caption layout options;
- generic floatrow `\floatbox` commands preserve nested images and captions
  while suppressing float type and layout options;
- floatrow `\ttabbox` commands preserve nested tables and captions while
  suppressing floatrow width macros;
- legacy `epsf` sizing assignments are threaded into following `\epsfbox` /
  `\epsffile` image options without leaking assignment tokens;
- legacy `\epsfig` / `\psfig` `file={...}` / `figure={...}` path options resolve
  brace-wrapped extensionless assets without leaking those option payloads into
  display-list text;
- `picins` `\piccaption` / `\parpic` pairs preserve inline picture assets and
  captions without leaking layout hints;
- `floatflt` `floatingfigure` / `floatingtable` environments preserve float
  contents while suppressing position and width arguments;
- `picinpar` `figwindow` / `tabwindow` environments preserve option-carried
  objects and captions while suppressing window placement arguments;
- `marginfigure` / `margintable` environments preserve images, captions, and
  labels through the figure/table capture path;
- `threeparttable` `measuredfigure` environments preserve images and captions
  through the figure capture path;
- `rotfloat` package shims preserve sideways float capture without
  missing-package diagnostics;
- `fancybox` image wrappers preserve nested graphics without undefined-command
  diagnostics;
- `psfrag` replacement helper commands no longer leak tags/options/replacement
  text around preserved graphics;
- `pstricks` `pspicture` environments emit unsupported-picture placeholders
  instead of drawing command text;
- LaTeX `picture` environments emit unsupported-picture placeholders instead
  of drawing command text;
- table horizontal rules now produce renderer-visible display-list rule ops;
- simple and repeated table column alignment specs survive into display-list
  text;
- simple vertical table border markers now produce renderer-visible
  display-list rule ops, including repeated `||` approximations;
- common `booktabs` spacing/rule-control commands are normalized without
  leaking command names or rule dimensions into table text;
- common `hhline` rule commands are normalized without leaking command names,
  pattern strings, or simple modifier payloads into table text;
- common `arydshln` dashed rule commands are normalized without leaking command
  names or dash/gap options into table text;
- common `colortbl` / `xcolor` table color commands and color-definition
  commands are normalized without leaking command names or color arguments into
  table text;
- common font declaration commands are normalized without leaking command names
  or font-size arguments into body/table text;
- common `longtable` repeated head/foot delimiters and template rows are
  suppressed from visible table fallback text;
- simple multirow row counts survive into `TableCell.row_span` metadata;
- broader production SVG/PDF vector embedding remains pending beyond the
  current simple SVG shape/path/group-transform/transformed-primitive subset.

Exit criteria:

- figure-heavy cases no longer show large blank image regions;
- tables are readable in raster output;
- extracted caption/table text is present.

### Phase 4: Math Subset

Scope:

- inline/display math IR;
- common symbols and operators, with first normalized-text slice implemented;
- superscript/subscript/fraction/root/accent/delimiter subset;
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

Current status:

- `PageDisplayList` page identities now use page content hashes plus page
  geometry and same-content occurrence counts instead of absolute page indexes,
  so unchanged pages can keep identity when earlier pages are inserted while
  duplicate same-content pages remain distinct.

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
- revtex affiliation/keywords/PACS;
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

1. Run the CC0 oracle with `ir_structure_slices` and use the lowest front
   matter, caption, reference, and fallback slice ratios to pick the next
   semantic recovery fixes.
2. Implement front matter capture and `\maketitle` output for `article`, `llncs`,
   `IEEEtran`, `revtex4-2`, and `wacv` semantic shims.
3. Change citation rendering so raw citation keys never enter visible PDF text.
4. Add `.bbl` bibliography rendering into internal output.
5. Introduce a minimal `Document IR` crate/module behind the current string
   output path, initially mirroring paragraphs/headings/title blocks only.

This order is intentionally front-loaded with measurable text recovery before
large layout work. It should raise oracle quality quickly while creating the
interfaces needed for real page layout.
