# VM Semantic Foundation Plan

This document defines the direct implementation plan for making one TeX
execution authoritative for events, semantic IR, checkpoints, and later math
layout.

The plan is based on a static review of `main` on 2026-07-26. It does not claim
that the full test suite or the arXiv corpora were executed as part of that
review. Every implementation batch below begins with focused failing or
characterization tests and ends with the relevant broader regression lane.

## Decision

The current crate boundaries are retained:

```text
tex-vm
  -> tex-render-model
  -> tex-layout
  -> tex-checkpoint
  -> renderer backends
```

The stable ideas that remain are:

- serializable render events and semantic document IR;
- source provenance and expansion frames;
- explicit fallback values rather than silent deletion;
- preamble, shipout, and input-boundary checkpoint categories;
- renderer state kept out of VM checkpoints;
- semantic IR kept out of direct VM mutation.

The authoritative execution path changes:

```text
Source files
  -> InputStack
  -> streaming Mouth
  -> Expander
  -> Executor
       +-> SemanticSink
       +-> TraceSink
       +-> DiagnosticSink
  -> RenderEvent stream
  -> SemanticDocumentIr
  -> LayoutIr
  -> PageDisplayList
```

The whole-source event scanner is no longer an authoritative producer. It
becomes a bounded recovery adapter invoked only for source that the VM has
actually reached and executed.

## Direct Work Policy

This work is performed directly on the current branch. It is not organized
around review branches or pull requests.

- Add a failing or characterization test first.
- Make one bounded architectural or behavioral change.
- Run focused tests and the required broader lane.
- Commit the green batch with a conventional commit message.
- Do not mix mechanical file movement with semantic changes.
- Do not push unrelated user files or untracked local artifacts.
- Do not move to the next batch while its exit criteria are red.

The order in this document is the default direct commit order. A parallel lane
may proceed only when it owns disjoint files and does not weaken a preceding
invariant.

## Verified Architectural Gap

The intended pipeline says that VM execution produces render events. The
current production path instead has two interpretation lanes:

```text
                         +-> whole-source scanner -> RenderEvent -> DocumentIr
source -----------------+
                         +-> eager tokenization -> VM -> legacy state/output
```

This fork cannot be made generally correct by adding scanner patterns. A
whole-source scanner would need to reproduce macro expansion, conditionals,
registers, groups, catcodes, input handling, and package-defined commands.

More precisely:

- the pre-scan runs when event capture is enabled, and production rendering
  enables that path;
- the root file is eagerly tokenized before execution, while included files are
  eagerly tokenized when loaded, so the problem is per-file rather than one
  up-front tokenization of the entire project;
- tokens contain pathless offsets, and snapshot token restoration currently
  loses even those offsets;
- the current event ID is an emission counter, and restore does not preserve it;
- `VmSnapshot` stores substantial compatibility state but omits enough input,
  conditional, expansion, sink, and ID state that it is not a full
  continuation;
- `tex-vm/src/lib.rs` is about 52,200 lines in the 2026-08-09 working tree, including core
  execution, compatibility, and tests.

The first semantic invariant is therefore:

```text
visible SemanticDocumentIr content
==
content produced by commands that the VM actually executed
```

## Dependency Order

The core lane is serial:

```text
V0 characterization
  -> V1 mechanical module split
  -> V2 scanner quarantine and event metadata
  -> V3 Eqtb and SaveStack
  -> V4 streaming Mouth and token origins
  -> V5 macro/prefix/command and execution-mode model
  -> V6 VM-owned SemanticSink migration
  -> V7 Snapshot v2 and transactional sinks
  -> V8 SemanticDocumentIr v3 and LayoutIr
```

Other lanes depend on it:

```text
actual browser PDF/pages -------- independent after its artifact contract
font resolver ------------------- independent of V3-V6
VM-owned MathList ---------------- requires V3-V6
persistent CompilerSession ------- requires V7
incremental semantic/page reuse -- requires V7-V8
TeX math layout ------------------ requires V8 plus MathList/font metrics
```

### Current State Matrix (2026-08-12)

| Phase | Implemented slices | Phase exit | Blocking evidence |
| --- | --- | --- | --- |
| V0 | extensive divergence, continuation, event, IR, and corpus characterization | open | the complete V0 fixture/expected-failure map has not been re-audited against this plan |
| V1 | command/input/Eqtb/SaveStack/snapshot/sink/family modules and a bounded control-sequence Eqtb owner exist | open | `tex-vm/src/lib.rs` remains about 52,200 lines and owns major execution/state paths |
| V2 event contract and reconciliation baseline | serialized build-local `sequence`, producer/confidence metadata, typed origin validation and static guards for a closed first-party writer taxonomy, schema-v5 producer-tag compatibility fixtures, zero public raw-constructor paths, shared source-location overlap for seven reconciliation families, producer-independent unmatched insertion anchors for five families, explicit scanner recovery, table suppression for lexical/runtime false conditionals, and sequence-independent current semantic IDs exist | open | finish the explicit all-family suppression/expected-failure closeout; graphic equivalence is audited and deferred, while execution identity, public event identity, and shared diagnostics belong to later or separate streams |
| V3 | Count/Dimen/Skip/Toks/CatCode plus control-sequence meanings use Eqtb/SaveStack; legacy `scopes` snapshots are projected from that state | open | muskip/mathcode/delcode/font/box/remaining parameters and persistent root/hash are absent |
| V4 | streaming Mouth/cursor and continuation slices | open | source identity semantics/registry, file/revision-aware lexical origins, scoped command identity, interned expansion arena, snapshot capability, and validated `ExecutedSourceSlice` are absent |
| V5 | macro parameter/prefix/protection slices | open | unified `EngineState` and explicit `NestFrame` are absent |
| V6 | many execution-owned semantic-family vertical slices | open | whole-source scanner entry, remaining recovery families, and final identity separation remain |
| V7 | partial continuation/replay characterization | open | this is not Snapshot v2, transactional-sink completion, or persistent-session readiness |
| V8 | existing SemanticDocumentIr compatibility builder | open/not implemented as designed | no `StableEventId` or renderer-neutral `LayoutIr` boundary |
| Diagnostic stream | schema-v6 render diagnostics carry stable codes for every current writer and an exact missing-graphic asset join key; both compiler adapters consume typed data | open | severity, phase, recovery, related diagnostics, canonical ownership, and versioned VM/renderer/HMR/WASM adapters remain |

Design snippets below for `TokenOrigin`, `EngineState`, `NestFrame`,
`StableEventId`, state roots, and `LayoutIr` are target contracts, not current
implementation. Later vertical slices do not waive earlier entry gates. New
V6/V7 feature families are frozen while the state-ownership dependency spine
is closed; existing slices remain characterization evidence and may receive
bounded correctness fixes.

The numbered phases form a dependency DAG, not a total commit order:

```text
V2 sequence fix (`ba9424d`) -> V2 event-contract closeout
                                  |
                                  v
                         V4 identity semantics/registry
                                  -> lexical origins
                                  -> expansion + scoped command identity
                                  -> readers-first snapshot capability
                                  -> validated ExecutedSourceSlice
                                                    |
                                                    v
                                           V6 family migrations
                                                    -> scanner retirement

V2 sequence fix -> V3 independence proof -> bounded V3 ownership migrations

public event identity/schema -------- separate readers-first stream after V4
shared structured diagnostics ------- schema-v6 render slice; wider ownership/adapters open
path-based build dependencies ------- independent build/cache stream
```

V3 may proceed before V4 only through the mechanical independence gate below.
V3 and V4 changes that both touch snapshot code remain operationally serialized
even when their architectural dependencies are independent.

## V0: Characterization And Divergence Tests

Before moving code, add tests that expose the semantic fork and protect existing
working behavior.

Required fixtures:

```tex
% Local register assignment.
\count0=1
{\count0=2}
\the\count0

% Delimited macro arguments.
\def\pair#1,#2;{#2/#1}
\pair a,b;

% False conditional must not emit math.
\count0=0
\ifnum\count0>0
  $wrong$
\fi
$right$

% Macro-generated math must emit math.
\def\emitmath{$x^2$}
\emitmath

% Runtime catcode affects unread characters.
\catcode`\@=11
\def\foo@bar{ok}
\foo@bar
```

Tests are layered:

- VM state/output expectation;
- RenderEvent expectation;
- provenance expectation;
- snapshot/restore expectation where currently supported;
- an explicit expected-failure marker for behavior not fixed in V0.

V0 records existing event, IR, diagnostic, and checkpoint goldens before the
mechanical split. It does not bless semantically incorrect output as the final
contract.

Until V7 is complete, checkpoint reuse is restricted to boundaries whose
required continuation state is explicitly represented and tested. A
checkpoint that omits active input, conditional, expansion, group, or sink
state is marked unsafe rather than reused optimistically.

Exit criteria:

- each known divergence has a focused test;
- unsafe continuation reuse has a conservative gate and diagnostic;
- current supported behavior has characterization coverage;
- no behavior is changed in the characterization commit.

## V1: Mechanical `tex-vm` Split

Reduce `tex-vm/src/lib.rs` to a facade and move existing code without changing
public APIs, serialized schemas, event ordering, or behavior.

Initial module layout:

```text
tex-vm/src/
  lib.rs
  engine.rs
  command.rs
  input.rs
  mouth.rs
  expansion.rs
  macro_def.rs
  conditionals.rs
  nest.rs
  page_builder.rs
  eqtb.rs
  save_stack.rs
  grouping.rs
  assignment.rs
  registers.rs
  io.rs
  snapshot.rs
  diagnostic.rs
  semantic_sink.rs
  source_recovery.rs
  compat/
    mod.rs
    latex_commands.rs
    package_loader.rs
    shim_registry.rs
    builtins/
```

Rules:

- move tests with their owning module where practical;
- retain public re-exports from `lib.rs`;
- do not rename serialized enum variants in this batch;
- do not change event metadata in the same commit;
- place existing source scanner code behind `source_recovery`, but do not yet
  change its call sites;
- place package/class shims behind `compat`.

Exit criteria:

- public API and goldens are unchanged;
- focused VM tests and the standard workspace test lane remain green;
- no single replacement module becomes another monolithic dumping ground;
- `lib.rs` contains facade, shared public types, and module wiring only.

Longer-term crate splits such as `tex-engine-core`, `tex-latex-profile`,
`tex-compat`, or `tex-source-recovery` remain optional. They are not performed
until module boundaries prove stable.

## V2: Scanner Quarantine And Event Metadata

### Producer Contract

The long-term versioned taxonomy target remains:

```rust
pub enum EventProducer {
    Primitive,
    Macro,
    CompatCommand,
    Shim,
    BblParser,
    ScannerRecovery,
    Fallback,
    Unknown,
}
```

Schema v5 deliberately has a narrower sanctioned writer image. Opaque
`EventOrigin` construction currently emits `Primitive`, `Macro`,
`ScannerRecovery`, `Fallback`, or `Unknown`. The public serde enum additionally
accepts and re-serializes `Command`, `Shim`, and `BblParser` for compatibility;
those tags are not current producer implementations.

Production writes use the opaque `EventOrigin` policy boundary and a
private-field `EventBuildContext` so an origin-unknown event cannot silently
receive a high-confidence default:

```rust
RenderEventEnvelope::try_from_origin(
    event,
    EventBuildContext::new(sequence, source),
    EventOrigin::primitive(),
)?
```

This boundary landed in `f06bcdf` and rejects event-kind/origin mismatches.
`EventOrigin` constructors own the valid producer/confidence projections;
callers do not assemble an unrestricted raw pair. Scanner `RawFallback` and
`Diagnostic` behavior is characterized and preserved by focused model tests.

The older incremental migration bridge was
`RenderEventEnvelope::with_origin(sequence, event, source, producer,
confidence)`. It landed in `525607a`, and executed list-item emission moved to
it in `43ba5de`; executed environment begin/end emission followed in `e6bb5a3`.
Executed inline citation, reference, label, and link emission followed in
`51aef83`; loss-aware caption emission followed in `7229c69`. Those migrated
families now use the typed boundary: list/environment in `e8840b2`, inline in
`a9ae789`, caption in `e4a5cdb`, and heading in `943e580`. Active heading, caption,
and footnote snapshot producers became fail-closed in `6205f9d`; footnote and
math writes then moved to the typed boundary in `c94664b` and `7edfd8b`, with
graphic following in `75f0803`. Active text snapshot validation landed in
`1baa07b`; front matter and text writes moved in `5fa5a5c` and `081248e`. The
table snapshot boundary and write moved in `70090a6` and `a3562f7`.
Bibliography snapshot validation, typed projection-loss origins, and its write
migration landed in `9fef0b7`, `9d122b0`, and `69e75ea`. After every call site
was classified and incidental fixture migration completed, public `new()` and
`with_origin()` were removed together (`0940368`). This is a Rust source API
break: ordinary callers migrate to `try_from_origin()`, scanner recovery uses
`from_scanner_recovery()`, and a mode override is applied afterward with
`with_mode_hint()`. JSON fields, producer/confidence tags, the legacy
`event_id` alias, and permissive serde reads are unchanged; a fixed legacy
Macro/Medium JSON fixture covers that read contract.

The syntax-tree policy now admits only `try_from_origin()` and
`from_scanner_recovery()` as public associated construction paths and allows
private `from_metadata()` assembly only from `try_from_origin()`. Existing
Clippy `disallowed-methods` remain as defense in depth (`776d604`, `0940368`).
The policy also rejects direct assignments to producer, confidence, and
provenance generated-by fields. It also rejects production expression
construction of the three v5 compatibility-only producers and a blanket
`From<GeneratedBy> for EventProducer` conversion, while allowing explicit
pattern matching that rejects decoded compatibility values. This protects
sanctioned first-party write paths; it does not claim that every representable
`RenderEventEnvelope` is valid, nor prevent external construction while its
fields and permissive serde remain public (`ba887bf`).
Table raw-fallback promotion and text leading-space reconciliation now rebuild
envelopes through typed origins while preserving sequence, source, and mode
(`75a79d5`). List, environment, heading, caption, graphic, front-matter, and
bibliography now share `source_locations_overlap()` (`decccd7`). The matching
identity uses only strict half-open file-span overlap from primary, every
related role, and expansion call/definition locations; it ignores
`generated_by`, producer/confidence, and truncation metadata. Inline, text, and
footnote retain their intentionally different span sets. Including definition
spans preserves existing behavior but can cross-match repeated macro invocations
that share a definition; this remains part of `ARCH-007` until an execution
identity or bounded `ExecutedSourceSlice` can replace coarse overlap.
Heading, caption, graphic, and front-matter unmatched insertion now share a
source-only terminal-call → related-Invocation → primary anchor (`694a0ee`). A
permissively deserialized heading whose source is identical but producer differs
proves insertion order is producer-invariant while preserving the complete
envelope. Bibliography keeps its distinct expansion→primary anchor but no longer
consults producer; valid Macro and lossy Fallback origins with identical
provenance now insert identically (`4c24516`). Graphic candidate path
equivalence still consults producer. Its audit found no safe change before
execution identity because current matching must retain scanner wrapper options
while preventing macro override cross-matches. The follow-up Pro review also
rejected a partial `ExecutedSourceSlice` until file/revision/expansion identity
lifecycles exist, and kept sequence/source reuse family-local.

Policy:

- VM primitive or verified macro path: high confidence;
- compatibility command with defined semantics: high or medium, declared by
  the adapter;
- scanner recovery: medium or low;
- lossy fallback: the current serialized `Fallback`/low projection remains
  compatible until a separate consumer and reconciliation audit approves a
  semantic change;
- a no-op shim emits a diagnostic and is not counted as supported behavior.

The serialized producer variant remains `Command`. A repository audit found no
sanctioned production assignment for `Command`, `Shim`, or `BblParser`; schema-v5
full-stream fixtures preserve all three exact wire tags, and active semantic
captures reject all three after deserialization. Renaming `Command` to
`CompatCommand`, assigning future `Shim`/`BblParser` semantics, tightening
legacy deserialization, and normalizing lossy or diagnostic taxonomy remain
separate migrations. A future wire migration requires a concrete producer and
consumer invariant plus a readers-first, rollback-safe version plan (`ba887bf`).

### Recovery Scope

The final recovery scanner must not scan the whole file and feed a second
production event stream into IR.

Illustrative V4 target; its exact ownership and durable representation require
the identity ADR described below:

```rust
pub struct ExecutedSourceSlice {
    context: IdentityContextId,
    source: SourceVersion, // file + exact immutable source revision
    span: ByteSpan,        // lexical origin of the executed command token
    command: ScopedControlSequenceId,
    expansion: ExpansionId,
}
```

The VM creates this slice only after reaching the construct through normal
execution. The scanner may recover a bounded command, argument, environment,
or math region from that slice. Expansion records expose named call,
definition, and argument anchors rather than letting consumers reinterpret the
direct lexical span. Top-level execution needs a truthful root occurrence; it
must not be encoded as a missing, zero, or placeholder expansion ID.

This is the required recovery contract, but its concrete identity-bearing type
must not be introduced with placeholder file, revision, command-context, or
expansion IDs. V2 keeps the scanner boundary and contract explicit. V4 must
establish the source registry, exact revision lifecycle, scoped interner
identity, expansion arena, and snapshot capability before it introduces this
private validated interface. `ExecutedSourceSlice` remains internal in V4 and
does not change schema v6. V6 owns the family-by-family migration and removal of
the authoritative whole-source production stream. Successful execution paths
use `ExecutedSourceSlice`; truthfully classified dispatch/failure paths require
a separately reviewed bounded input instead of a weakened slice.

During V2-V6 migration, the old whole-source scanner may temporarily remain:

- in debug artifacts;
- in migration differential tests;
- as an explicitly low-confidence compatibility source for event families that
  have not migrated yet.

No new production feature is added to that whole-source path. Each V6 vertical
slice removes one event family from the whole-source recovery bridge. The bounded
execution contracts are the only remaining production recovery paths at final
V6 exit; they may not rescan the whole source. False-conditional leakage must be
removed by family-specific suppression tests or remain an explicit failing
characterization until the corresponding family migrates; it must not be
hidden by a high-confidence event. Table recovery now covers both lexical and
runtime-false conditional branches in `crates/tex-vm/tests/semantic_table.rs`.
Runtime-false `minipage`
layout-container pairs are registered with environment reconciliation and
discarded through the same suppression ranges, while visible pairs remain
covered (`e69cb6d`). False `DocumentClass` recovery uses the same bounded
filter and retains the actual class (`7e24b0a`). Package-derived
`SetDocumentLayout` now joins that suppression-aware family. The eager scanner
no longer mutates a class payload in place; reconciliation reapplies the
NeurIPS `10pt` projection only from a surviving scanner layout that follows a
surviving class. The runtime-false regression verifies no layout event, no
class-option mutation, default Document IR layout, and visible body retention
(`f37fd97`, resolved `BUG-025`). Scanner `\twocolumn` and `\onecolumn` commands
also emit source-scoped layout intents instead of mutating the class eagerly.
Their event IDs are snapshot-aware and survive recovery refresh/remap, while
only unsuppressed events project the corresponding column option into the
surviving class. Focused false/visible regressions and an input-exit continuation
round-trip cover the event, class option, and Document IR column count
(`a3a39a0`). Direct primitive graphic reconciliation also rejects scanner-only
`graphicx` package-mode prefixes such as `draft` when their package invocation
was runtime-false. The bounded rule preserves scanner-owned resize/scale wrapper
suffixes and visible package defaults (`126f383`). Scanner-side `Gin` defaults
now carry source and execution-occurrence contributions through snapshots.
Graphic reconciliation removes only runtime-false contributions with
brace-aware option matching, preserving visible and duplicate-equal defaults,
local wrapper options, recovery refresh, event-ID remapping, and input-exit
continuation replay (`d5714b7`). Scanner recovery diagnostics for missing
input/package/class and cyclic input now join the same
suppression-aware family. A runtime-false missing input leaves no diagnostic
event, while the visible `Unknown`/low event remains covered and input-exit
continuation replay preserves the filtered result (`2348ff5`).
Non-table `RawFallback` recovery now uses that suppression-aware environment
family as well, so an unknown environment in a runtime-false branch cannot leak
fallback output. Table fallbacks deliberately stay on their table-start anchor
path because child phantom/spacing suppression ranges must not discard a visible
table (`01b3634`).

Algorithmic `\If`/`\Else`/`\Comment` adapters now register their manually
emitted scanner `Text`/`Space` with invocation-aware scanner slots. Their
runtime-false prefix, suffix, note, and explicit space are removed while the
following visible commands remain once (`cba90eb`). The ensuing producer audit
found that this is not yet an all-family result: theorem optional titles,
inline formatting/link/unit helpers, escaped space/symbol commands, `\xspace`,
and overpic overlay text still have manual scanner `Text`/`Space` writers that
bypass scanner transaction ownership. V2 therefore keeps these paths explicit
and open rather than treating the structurally registered event families as
proof for their nested text children.

A mixed macro regression now places a runtime-false prefix before a visible
lossy caption argument in the same invocation. No scanner caption exists for
that shape, so the VM's executed `Fallback`/low caption is the only truthful
event. Executed caption captures have already passed VM control flow and are no
longer discarded by coarse source suppression; a companion false-branch macro
test proves that an unexecuted caption is still absent (`4520dfd`). Graphic,
bibliography, and raw-fallback mixed cases remain open under `ARCH-007`.

Simple inline wrappers now register their direct scanner text with an
invocation-aware slot. Runtime-false `\emph{Wrong}` is discarded, while the
visible control appears exactly once and is reconciled from
scanner-recovery/medium to the existing primitive/high execution event. The
provenance golden records that authority change (`829bb34`). Wrapper branches
with nested commands remain in the manual-writer inventory. The first nested
case now emits and owns `\nolinkurl` recovery text atomically against the inner
URL invocation rather than the outer formatting wrapper. A runtime-false nested
URL is discarded and the visible occurrence reconciles to primitive/high
(`6055469`). Nested simple formatting uses the same inner-invocation ownership,
so runtime-false `\emph{\textbf{Wrong}}` is discarded while the visible nested
text reconciles to primitive/high (`ca1f6fb`). The helper is typed over only
`Text` and `Space`; nested escaped symbols and control-space now use their exact
token ranges, removing the hidden occurrence while retaining the visible
scanner-recovery/medium event (`78e0b1d`). A nested unknown command with a
simple braced visible argument now owns that fallback text against the unknown
command invocation; the hidden argument is removed while the visible argument
remains once (`c6a45be`). Nested `\href` text inside `NoHyper` now uses the
same atomic inner-invocation ownership, removing the runtime-false label while
retaining one visible text event and no link event (`bed136f`). Nested no-link
`href` inside an unknown-command fallback now has the same exact ownership and
false/visible regression (`097d43d`), and the parallel deeper `url` branch now
does as well (`399e2b4`). The nested inline inventory is closed for these
direct writers. Non-graphic `\textcolor`/`\colorbox` and `\fcolorbox` text now
uses the same atomic invocation ownership; runtime-false color text is removed
while both visible containers remain once (`3a836b1`). Float-local minipage
body text inside figure/table recovery now owns only its exact plain-text range;
the runtime-false body is removed while the visible occurrence remains once
(`1a0665c`). The remaining direct `Text`/`Space` writer audit stays open rather
than treating these bounded families as proof for every recovery branch.

Theorem-like optional titles and their following interword space now form one
scanner text transaction. A runtime-false theorem loses its title, body, and
block events together, while the visible theorem retains each once
(`7c8f2ab`).

The siunitx `\SI`/`\qty`/`\num`/`\si`/range/angle adapters now register their
manual text against the full command invocation. Runtime-false quantity/unit
text is removed and the visible control remains once (`6ac0dde`).

The `\hyperref`, `\hyperlink`/`\hypertarget`, and
`\nolinkurl`/`\path`/`\detokenize` direct-text paths now use the same
invocation-aware ownership. Runtime-false labels and URLs are discarded while
visible helpers remain once; executable braced `\path` text is promoted to its
primitive/high event and the provenance golden records that contract
(`50bd7d0`).

Escaped text symbols, explicit control-space, and `\xspace` now register each
manual scanner event against its invocation. Runtime-false helper events are
removed while the visible `%`, explicit space, and `\xspace` each remain once
(`17d287e`).

The threeparttable `\tnote` adapter now registers its bracketed marker text
against the invocation. A runtime-false marker is removed and the visible
marker remains once (`623df34`).

Overpic `\put`/`\multiput` overlay text now registers against the bounded
overlay invocation. A runtime-false overpic loses both its graphic and overlay
text, while the visible primitive graphic and overlay text survive
(`5f6b9a3`).

A Pro V2 closeout review returned `REVISE`: it accepted occurrence-aware
per-writer ownership and the rule that committed executed captions must not be
discarded by coarse macro-call suppression, but rejected an outer-wrapper
transaction for nested text. V2 therefore uses a typed atomic
emit-plus-ownership helper with the narrow inner invocation, keeps structured
child events outside text ownership, and requires repeated-input/repeated-macro
caption identity plus executed-caption lifecycle evidence before changing the
gate to green. The caption advice remains an audit target rather than an assumed
snapshot-schema change; a RED repeated-occurrence test decides whether the
existing global scanner anchor map is sufficient or caption-local anchor state
is required.

A subsequent Pro design review rejected a generic scanner graphic-state
mutation log: package loading, pending-option routing, and other deferred state
would turn recovery into a second TeX engine. The loaded `overpic` shim now uses
the executed `\begin` boundary to consume its local options and backing target,
then resolves a primitive graphic from current VM path, extension, and default
state. Runtime-false extension and arbitrary package defaults therefore cannot
contaminate the visible wrapper, visible controls remain authoritative, and an
unloaded same-named environment keeps its body intact (`533d7ee`). If another
scanner state family can change event existence or requires package/deferred
semantics, the stop condition is to migrate that wrapper through a bounded
`ExecutedSourceSlice`, not add another generic mutation-log variant.

### Event Sequence

At this stage, rename the current meaning honestly:

```rust
pub struct EventMeta {
    pub sequence: u64,
    pub source: SourceProvenance,
    pub mode: SemanticMode,
    pub producer: EventProducer,
    pub confidence: SemanticConfidence,
}
```

`sequence` is build-local ordering. Do not derive `StableEventId` yet: it
cannot be defined correctly until V4 supplies file-aware token origins and
interned expansion records. Revision-aware provenance and typed zero/one/many
dependencies belong to the later readers-first event-schema stream; they are
not singular fields assumed here. Footnote or node identity must stop depending
on the next event sequence before replay reuse is enabled.

Footnotes now own a separate monotonic `FootnoteId` allocator, and changed-input
recovery no longer infers that identity from the event sequences reused by the
sink transaction. On a successful source refresh, the live footnote namespace
is densely rebased to the same allocation phases as a clean build: recovery
scanner identities in final stream order, followed by the pre-refresh executed
transaction/active/pending identities in allocator order (`ba9424d`). The
refresh path asserts that rescanning did not create executed footnote state.
Regressions cover an earlier non-footnote insertion, child-note insertion and
deletion before a later parent note, and an active state-only footnote crossing
the changed input; the later checkpoint allocator snapshot must equal a clean
run.

A Pro review correctly challenged the initial unqualified state-only suffix
assumption. Repository tracing showed the missing lifecycle fact: recovery
refresh runs only the scanner before loading the module, so it cannot add a
completed transaction, active capture, or pending executed mark. The
implementation enforces that boundary instead of introducing the review's
broader anchor planner. `FootnoteId` remains build-epoch-local and may be
renumbered after an earlier source edit. Current layout/IR consumers are built
from the completed event stream and do not survive inside the VM refresh
transaction; cross-revision stable identity remains deferred with
`StableEventId`.

### Legacy Constructor Inventory (2026-08-11)

All 112 call sites present before this migration slice were classified. After
all 12 production writes, three origin-sensitive semantic-text fixtures, two
synthetic semantic-sink fixtures, one golden fixture, one compiler fixture, and
63 layout fixtures plus six model serialization fixtures moved to typed
construction, the 24 constructor-contract calls and both raw APIs were removed
together in `0940368`:

| Class | Count | Policy |
| --- | ---: | --- |
| public raw constructor definitions | 0 | structural policy review-gates any new associated constructor |
| real `new()`/`with_origin()` call expressions | 0 | syntax-tree and Clippy CI guards keep this invariant |
| incidental test fixtures | 0 | all classified fixtures now declare typed origins |

Table now validates restored frame producers before typed construction.
Bibliography validates restored captures and uses distinct
`primitive_with_projection_loss()`/`macro_with_projection_loss()` origins, so
authoritative execution with a lossy string projection is not collapsed into
the generic `Fallback`/low origin. The table reconciliation path that converts
an existing `RawFallback` payload into `Table` now reconstructs the envelope
with scanner-recovery/medium origin, preserving scanner sequence, source, and
mode while adding the executed expansion stack only when the scanner source
lacks one. The analogous text leading-space promotion reconstructs a
primitive/high envelope without direct origin metadata mutation.

The origin-sensitive semantic-text fixtures moved in `91a8daa`, and the
synthetic semantic-sink fixtures now declare unknown/low origin in
`edbe93c`. Golden text declares unknown/low while the compiler diagnostic uses
diagnostic-unknown in `dc72656`. The layout fixtures moved in `247a647`: 62
ordinary synthetic events declare unknown/low, while the sole `RawFallback`
uses its required fallback origin. The final six model serialization fixtures
declare unknown/low in `80ca0e2`. Parsed legacy-call examples in the syntax
guard self-test are source strings, not live compatibility-constructor call
sites, and are excluded from the inventory. Serde reads bypass constructors,
so legacy Macro/Medium wire input remains accepted and is covered by a fixed
JSON fixture even though Rust callers can no longer use the raw APIs.

### V2 Evidence Matrix (2026-08-12)

| Contract | Current evidence | State | Remaining gate |
| --- | --- | --- | --- |
| build-local sequence and schema migration | schema v6 serializes `sequence`, accepts legacy `event_id`, and the sink snapshots its next/batch sequence; fixed schema-v5 fixtures remain readable | green | keep it out of revision-stable identity |
| scanner producer/confidence | typed scanner construction preserves ordinary `ScannerRecovery`/medium, `RawFallback` fallback origin, and current diagnostic `Unknown`/low behavior in focused model tests | green | preserve these semantics on every bounded recovery path; any diagnostic retag is a separate change |
| primitive/macro origin | all current production `new()` writes and direct producer/confidence/generated-by mutations have migrated; executed list, environment, inline, caption, heading, footnote, math, graphic, front-matter, text, table, and bibliography paths pass an opaque typed origin into construction | green | preserve the syntax-tree and Clippy guard invariant for new families |
| explicit constructor contract | opaque `EventOrigin`, private-field `EventBuildContext`, and `try_from_origin()` reject event-kind/origin mismatches; both public raw constructors and all real calls are gone; structural policy admits only the typed/scanner public paths and limits the private assembler; fixed JSON proves permissive legacy reads remain | green | preserve the sanctioned-path invariant without conflating it with full representational validity or wire-read strictness |
| sanctioned production-write taxonomy | exhaustive typed-origin tests map every current writer to `Primitive`, `Macro`, `ScannerRecovery`, `Fallback`, or `Unknown`; the AST policy rejects direct compatibility-only construction and provenance-to-authority conversion in production source | green | preserve this closed first-party writer image until a concrete new authority is designed |
| schema-v5 producer compatibility | full-stream fixtures deserialize and reserialize `command`, `shim`, and `bbl_parser` without relabeling or changing schema 5; active semantic captures reject all three | green | keep decode/round-trip compatibility separate from consumer and snapshot-state validity |
| future producer semantics | no sanctioned origin, production assignment, or consumer invariant exists for `CompatCommand`, `Shim`, or `BblParser` | deferred | require a real producer-plus-consumer contract and an explicit readers-first/rollback-safe schema decision before any rename or new wire tag |
| false-conditional isolation | lexical and runtime-false table recovery, scanner-only minipage layout-container pairs and float-local layout text, false `DocumentClass`, layout/class-option projections, graphic package/default state, diagnostics, non-table raw fallbacks, algorithmic text, simple plus nested-URL/nested-formatting/nested-symbol-space/nested-unknown/nested-NoHyper-href/deep-NoHyper-link-and-URL inline-wrapper text, color-container text, theorem optional titles, siunitx/link/spacing/tnote/overpic-overlay text, and mixed lossy-caption execution have bounded regressions | partial/open | finish the remaining direct manual `Text`/`Space` writer audit; characterize repeated-occurrence caption identity and capture lifecycle, representative replay, all diagnostic subtypes, and remaining `ARCH-007` graphic/bibliography/fallback mixed macros whose visible child follows a skipped prefix |
| reconciliation location identity | seven families share a source-only overlap contract; heading/caption/graphic/front-matter share a terminal-call→Invocation→primary unmatched insertion anchor, while bibliography uses a source-only expansion→primary anchor; producer-invariance regressions cover both shapes | partial | keep graphic path equivalence and repeated-macro definition-span matching unchanged until execution identity exists; keep narrower inline/text/footnote rules separate |
| bounded recovery input | `ExecutedSourceSlice` exists only as a target contract; a Pro review rejected placeholder identities and compiler-side decoration | moved to V4/V6 | V4 constructs the validated internal handle after identity-complete snapshot support; V6 migrates consumers family by family |
| path-based build dependencies | the compiler tracks loaded sources plus final reconciled `GraphicRef`/`IncludePdf` paths; a visible missing asset is tracked while a runtime-false asset is excluded (`9ed7a09`) | independent/partial | continue build read-set coverage independently; never derive semantic `DependencyId` from path order or claim this satisfies event identity |
| public event revision/dependencies | current schema-v6 `EventMeta` does not carry them, and one event may span several source revisions or dependencies | separate schema stream | after V4, decide whether revision belongs on each provenance reference and model zero/one/many typed dependencies before a readers-first version migration; do not assume singular `EventMeta` fields |
| shared structured diagnostics | schema v6 gives every current render-diagnostic writer a stable code and carries the exact missing-graphic `asset_ref`; both compiler adapters use the validated typed pair rather than parsing the display message, while schema-v5 message-only diagnostics remain readable and round-trip as `Unknown` without semantic inference | separate architecture stream/partial | choose a dependency-neutral canonical owner, then add audited severity/phase/recovery and versioned VM/renderer/HMR/WASM/snapshot adapters; keep EPS projection on renderer state rather than duplicating raster/PDF policy |
| sequence-independent semantic identity | footnotes use an independent allocator; changed-source replay densely rebases scanner and executed identity phases without consulting event-sequence correspondence, including active state-only and note-count-change regressions; a repository-wide audit found no other emitted payload/IR identity derived from `EventSequence`, while sink, snapshot, scanner, bootstrap, and compiler uses remain ordering or transaction correlation only | green | forbid new semantic IDs derived from sequence or byte offsets; keep cross-revision identity deferred to V4-backed `StableEventId` |
| `StableEventId` | intentionally absent until V4 file-aware token/expansion origins | correctly deferred | add only after the V4 prerequisite is green |

Exit criteria:

- scanner events are visibly distinguishable in JSON/debug artifacts;
- no public raw compatibility constructor bypasses typed origin validation;
- every known recovery family has a suppression regression or an explicit
  low-confidence expected failure for false-conditional leakage;
- every manual scanner `Text`/`Space` producer is owned by an execution-aware
  scanner transaction, including continuation replay and mixed macro cases;
- the current `event_id` contract is migrated/versioned as build-local
  `sequence`;
- no code treats sequence as revision-stable or semantic identity;
- current producer-coupled reconciliation cases are characterized and listed as
  deferred risks rather than silently generalized;
- V2 does not define, construct, serialize, or consume `ExecutedSourceSlice`,
  add a schema-vNext writer, or add production features to the whole-source
  scanner.

File/revision/expansion identity, validated `ExecutedSourceSlice` construction,
public event identity, and shared diagnostics are not V2 exit criteria. They
remain explicitly tracked by the V4/V6 or separate-stream gates above.

## V3: Eqtb And SaveStack

Replace split control-sequence scopes and independent register maps with one
assignment model.

### V3 Independence Gate

The next bounded V3 batch proves independence before it moves production
ownership. V3 may proceed before V4 only when all of these checks are green:

- production changes do not reference file/source revision, expansion,
  `ExecutedSourceSlice`, source paths/spans, compiler build revision, or
  `EventMeta.sequence` as identity;
- the existing interner-local `ControlSequenceId` remains local and is not made
  cross-run durable;
- schema-v6 fields/tags, schema-v5 compatibility fixtures, HMR/WASM wire types,
  and checkpoint format versions do not change;
- internal snapshot ownership may move only while its serialized representation
  and legacy fixtures remain equivalent—byte-identical where deterministic,
  otherwise decode-equivalent under the same format version;
- nested local/global assignment, `\globaldefs`, group unwind, snapshot,
  restore, continuation, and replay produce equivalent Eqtb/SaveStack state,
  events, diagnostics, and recovery-visible behavior against the post-`ba9424d`
  baseline;
- a changed-path/added-symbol guard rejects incidental identity, public-schema,
  and checkpoint-format work in a V3 ownership batch.

The gate fails if control-sequence migration requires a persisted field, changes
command-ID lifetime, consumes source provenance, or changes replay/event output.
That work must then be coordinated with V4 or a separately reviewed snapshot
migration. V3 and V4 edits to `snapshot.rs` are serialized operationally even
when the gate proves their architecture independent.

The entry gate and the first production-owner migration are green:

- `2289907` adds a self-tested CI diff guard. A commit that touches a V3 owner
  file may change only the bounded VM production surface plus tests/plans, and
  added production lines may not introduce execution identity, provenance,
  durable command identity, or serialization symbols.
- `fe6b4df` pins continuation-safety version 2, semantic-capture version 22, and
  the existing `VmSnapshot.scopes` JSON shape with a normalized golden, full
  JSON decode, fresh-interner restore, and exact control-sequence behavior.
- `d9cdf02` compares nested local/global shadowing, positive/negative
  `\globaldefs`, group unwind, scopes, output, events, diagnostics, transcript,
  registers, visible graphic recovery, and false missing-input suppression
  across a clean run and an input-exit JSON checkpoint replay.
- `f66cdbf` fixes the prerequisite TeX rule that a same-name global control-
  sequence assignment cancels pending local restores at every open group.
- `c640efb` adds the common Eqtb key/value, SaveStack restoration, borrowed-name
  lookup, and legacy-layer projection APIs.
- `775cc22` removes the parallel `ControlSequenceScopes` production owner.
  Control-sequence definitions, `\let`, direct-global kernel helpers, group
  unwind, depth checks, and lookup now use Eqtb/SaveStack. The existing
  `VmSnapshot.scopes` field is reconstructed without a new persisted field or
  version change, and fixtures cover macro, primitive, token, fresh-interner,
  checkpoint replay, and unsafe open-group restoration.

The post-migration Pro review is remediated in bounded commits:

- `5ca539e` and `b4c2695` compare exact layers and visible lookup against an
  independent layered model for every valid two-name sequence through length
  seven and depth three. The generator proves that repeated equal meanings at
  distinct levels are exercised;
- `f9f23ef` projects legacy layers by simulated unwind and rejects restore
  records whose current/root/previous levels violate the representation
  invariant;
- `824b51c` boxes only control-sequence meanings, keeping `EqValue` at the same
  24-byte size as a register-only comparison enum while the dedicated string
  map retains allocation-free borrowed lookup;
- `b4d1500` replaces canonical author-separator mutation with a lexical
  full-expansion protection list that propagates through nested `\expanded`
  and `\expandafter` processing;
- `340ea72` reconstructs legacy open groups as control-sequence-only restore
  frames, preserving the wire format's historical lack of register restore
  history;
- `99a55ae` rejects a rootless `scopes = []` before restore construction,
  preserves valid empty layers exactly at depths 1, 4, 1000, and 1001, and pins
  unsupported meaning rejection at decode time. `Meaning`/`SnapshotMeaning`
  have no explicit `Undefined` variant, so absence remains the distinct
  `Option<EqEntry>::None` state rather than a value that projection could erase.
- `d58f4e0` makes an untrackable register save in a reconstructed legacy
  control-sequence-only frame explicit as `SaveDisposition::UntrackedLegacyFrame`.
  `Eqtb::assign` stores that persistent write at effective level zero, and the
  regression verifies both legacy unwind and a later ordinary local restore.
- `90a7628` adds `Vm::try_restore` and typed missing-root/unknown-primitive
  errors. Validation completes before VM construction or interner mutation, and
  all four production restore boundaries in `tex-bootstrap` propagate the
  fallible result. The asserting `Vm::restore` remains only as a trusted
  compatibility wrapper.

Group-end ordering was also audited. Source catcode-overlay cleanup after
`Eqtb::end_group` is a pure `retain` over source-frame maps and performs no
lookup, event emission, diagnostic, or callback, so restoring control sequences
inside the common Eqtb unwind is not observably reordered.
`scripts/check_v3_cross_version.py` now supplies the stronger runnable rollout
matrix: it builds detached `f66cdbf` and candidate worktrees and exchanges two
real snapshots in both producer-consumer directions. The second fixture added
in `e5a630e` projects `{x=R,y=G} / {x=L} / {} / {x=L,z=Z}`, combining an
equal-value repeated layer, an empty intervening layer, an absent predecessor,
and globally cancelled history. Consumers reproject before continuation; the
validator checks depth, exact selected keys/kinds, complete directional result
equality, zero diagnostics, and the discriminating unwind output
`LGZLGALGARGA`. This is a manual release gate rather than doubling the normal CI
build.

The bounded slice was validated in a clean detached worktree at exact commit
`cd64df66bc1b5bccea85b284ffaff7dfc080c1a5`: tex-vm library 660, the six
relevant integration targets 102, tex-bootstrap library 64, and Python
independence/matrix tests 11 all passed; the real-binary two-fixture
`f66cdbf <-> cd64df6` matrix, canonical workspace Clippy, and formatting check
also passed. The matrix script hash was
`9ad0c5ec00a80859e4c9529ba416ac567d8ab5c015b2442a8e700b804cac83fe`.
`cd64df6` additionally pins author-local lexical protection through a focused
`expandafter` regression without changing the canonical `and` meaning.

The final targeted Pro review
`6a7c6421-1a38-83ee-af57-7dee503ceced` returned **PROCEED** with 0.92
confidence for the control-sequence ownership slice. It accepted the explicit
legacy-frame disposition, exact layered mixed-version intersection, fallible
production restore boundary, lexical `expandafter` evidence, and exact clean
commit provenance. The verdict is scoped to this slice; resource admission,
generated public-wire properties, and the remaining V3 assignment classes are
still open.

The first absent assignment class, muskip, crosses a different boundary from
the closed control-sequence slice: the legacy `VmSnapshot` has neither muskip
state nor an allocation cursor. The targeted schema review
`6a7c6961-a170-83ee-be0f-746c526eb3ac` therefore returned **PROCEED** with 0.87
confidence only for a hybrid reader-first/runtime-only rollout. Durable muskip
checkpoint writing remains disabled.

The migration evidence and current boundary are:

- `e3bec73` builds the exact pre-reader `00c8ee3` binary. It proves that an
  additive field-only snapshot is accepted with muskip silently discarded; a
  nested versioned raw document is rejected; a versioned-only checkpoint is
  accepted but not replayed; and a dual-lane checkpoint replays only its legacy
  lane.
- `dcbee7c` centralizes prior-checkpoint reuse loads as typed missing/unreadable
  cache misses. The compiler skips incompatible earlier revisions or rebuilds
  from source, and a policy test forbids bypassing that boundary with the
  low-level loader.
- `1d29aaa` adds a reader-only semantic document with format
  `latexd.vm-snapshot`, independent schema version 1, typed open-ended
  capability names, legacy-flat normalization, and a two-stage decoder. It
  validates format, schema, and capabilities before strict state decoding, then
  offers decode plus fallible restore as one mutation-free error boundary.
  Unknown fields in the known document/state schema fail closed.
- The supported capability set is intentionally empty in this phase, so
  `eqtb.muskip.scalar-v1` is rejected before state decode. There is no document
  serializer, versioned checkpoint slot, or production writer yet. Legacy raw
  snapshots, checkpoint envelope schema 2, and production checkpoint bytes
  remain unchanged.

The exact remaining order is: add the dual checkpoint lane and a single-lane
internal attachment reader while keeping the writer `LegacyOnly`; add distinct
`MuSkip`/`MuGlue`/scalar-newtype Eqtb state and its independent cursor; make the
in-memory snapshot complete; add primitives and arithmetic; suppress attachment
for muskip-tainted state while the writer is disabled; implement the versioned
writer behind an explicit disabled policy; then activate only after old/new
real-binary gates and reader deployment. A capability-bearing state must never
be written through the legacy lane, and a checkpoint with both lanes is invalid.

The asserting production-restore follow-up is already closed by `00c8ee3`,
which makes `Vm::try_restore` the guarded production boundary.

The replay RED also exposed that semantic expansion markers split register
aliases such as `\globaldefs → \count251` from following assignment syntax when
event capture was enabled. `d9cdf02` makes count/dimen/skip/toks register-alias
expansion markerless and pins capture/non-capture assignment equivalence. This
is a prerequisite correctness fix, not a control-sequence ownership move.

```rust
pub enum EqKey {
    ControlSequence(String),
    Count(u16),
    Dimen(u16),
    Skip(u16),
    MuSkip(u16),
    Toks(u16),
    Box(u16),
    CatCode(u32),
    MathCode(u32),
    DelCode(u32),
    IntegerParameter(IntegerParameter),
    DimensionParameter(DimensionParameter),
    GlueParameter(GlueParameter),
    CurrentFont,
    FamilyFont {
        family: u8,
        style: MathFontStyle,
    },
}

pub struct EqEntry {
    pub value: EqValue,
    pub level: GroupLevel,
}

pub enum SaveEntry {
    GroupBoundary(GroupKind),
    Restore {
        key: EqKey,
        previous: EqEntry,
    },
    RestoreUndefined {
        key: EqKey,
    },
    AfterGroup(TokenId),
}
```

All assignment commands ultimately pass through one state-write API:

```rust
pub fn assign(
    &mut self,
    key: EqKey,
    value: EqValue,
    requested_scope: AssignmentScope,
) -> Result<(), Diagnostic>;
```

The command-family adapter resolves `\global`, `\globaldefs`, and any
family-specific policy before calling the API. The API saves the prior local
value once per group level and applies TeX-compatible global restore
suppression.

Migration order:

1. control-sequence definitions and `\let` — landed in `775cc22`;
2. count registers and arithmetic — landed;
3. dimen registers — landed;
4. skip registers — landed; muskip readers-first migration has completed the
   old-binary fixture, cache-miss normalization, and reader-only document phases;
5. token registers — landed;
6. catcodes — landed;
7. mathcodes and delcodes;
8. fonts, boxes, and remaining parameters.

Current migration evidence through `cd64df6`:

- `EqKey::ControlSequence(String)` and `EqValue::ControlSequence(Box<Meaning>)` use
  the common SaveStack save-once/global-cancellation path. Eqtb keeps a
  dedicated `BTreeMap<String, EqEntry>` so borrowed `&str` lookup does not
  allocate and interner-local command IDs do not become durable identifiers;
- SaveStack is the single group-depth and restore authority. The deleted
  `control_sequence_scopes.rs` no longer owns production state;
- package/class base-scope promotion remains control-sequence-only, including
  nested input inheritance and negative `\globaldefs`; register/catcode scope
  behavior is unchanged;
- builtin primitives remain a lookup fallback rather than seeded Eqtb entries,
  while `@cons` and `g@addto@macro` preserve their unconditional-global rule;
- author metadata applies lexical protection to visible macro meanings for
  `and`/`thanks` during full expansion, including nested expansion primitives,
  without mutating Eqtb or SaveStack;
- the serialized `VmSnapshot.scopes` shape and versions are unchanged. Eqtb
  state plus SaveStack restore groups project legacy layers, and restore
  rebuilds those groups root-to-leaf;
- restored legacy open-group frames accept only control-sequence restore
  records. The existing wire format carries no historical register restore
  chain, so post-restore register/catcode assignments in those already-open
  groups continue to survive group exit instead of acquiring new rollback
  behavior;
- legacy-layer projection validates the complete simulated unwind, and rootless
  snapshots and unknown primitives return typed errors before construction or
  interner mutation while valid empty and over-runtime-limit legacy layers
  retain their exact accepted depth;
- unsupported register writes in reconstructed legacy CS-only frames become
  canonical level-zero state, rather than persistent values carrying a stale
  group level;
- the exact four-layer real-binary matrix is green in both old/new directions,
  including repeated equal values, empty layers, absence, and global
  cancellation;
- migration step 1 is complete. The V3 exit remains open for the assignment
  classes and persistent state-root work listed below.

Non-blocking follow-up hardening for this closed slice:

- prevent future production-crate calls to the asserting `Vm::restore` wrapper
  with a compiler-assisted or CI source guard;
- extend malformed-restore atomicity coverage to a non-empty sentinel interner
  and an unknown primitive in a deep layer;
- route generated owner states through public serialization, restore, and
  projection, shrinking any mismatch into a deterministic fixture;
- measure restore duration and peak RSS by input bytes, scope depth, and binding
  count before proposing any versioned resource-admission limits.

Every migrated class gets:

- local assignment test;
- nested local assignment test;
- global assignment test;
- `\globaldefs` test;
- arithmetic/prefixed assignment test;
- snapshot/restore test;
- generated/property test for group nesting where practical.

Exit criteria:

- local register and catcode assignments restore correctly;
- every assignment primitive uses the common API;
- old per-type maps and scope-only restore code are removed after their final
  user migrates;
- Eqtb state can be hashed and referenced by a persistent root.

## V4: Streaming Mouth And Execution Identity

V4 begins with an identity ADR; no production ID type lands before it defines
source revision as the exact immutable normalized UTF-8 buffer indexed by
`ByteSpan`, the logical locator/file lifecycle, identity-context namespace,
fresh/continuation/mounted/restore/rebase behavior, command-interner scoping,
and the direct slice span meaning. Compiler build revision is a separate domain
and has no implicit conversion to source revision.

The runtime and snapshot model distinguishes two capabilities:

```text
LegacyPathOnly
  preserves old path/span behavior
  cannot construct ExecutedSourceSlice

IdentityComplete(context)
  owns one source registry, interner scope, and expansion arena
  may construct a validated ExecutedSourceSlice
```

An old path-only checkpoint remains `LegacyPathOnly` or triggers an explicitly
reviewed fresh rebuild. It never acquires file/revision/expansion identity by
inference. Readers for identity-complete snapshots must land before writers;
rollback-capable readers remain available while the new writer is enabled.

V4 implementation order is:

1. approve identity semantics and ownership;
2. register exact immutable source bytes at the resolver boundary in shadow
   mode, without compiler-side event decoration;
3. attach file/revision-aware lexical origins to source tokens;
4. add an occurrence-specific expansion arena and command identity scoped to
   the same execution/interner context;
5. add readers-first snapshot/restore capability and explicit legacy behavior;
6. construct the private validated `ExecutedSourceSlice`, rejecting mixed
   contexts, stale revisions, invalid spans, command mismatches, unrelated
   expansions, legacy capability, and scanner-origin attempts.

V4 keeps schema-v6 output stable by resolving internal records back to current
path/span/textual provenance. It does not migrate recovery consumers, introduce
semantic dependency identity, or consolidate diagnostic formats.

The main execution path stops calling whole-document `lex_plain()` before
execution. The VM asks for the next token at the current catcode state.

```rust
pub struct InputStack {
    pub frames: Vec<InputItem>,
}

pub enum InputItem {
    CharacterSource(InputFrame),
    TokenList(TokenListCursor),
}

pub struct InputFrame {
    pub file: FileId,
    pub revision: RevisionId,
    pub source: Arc<str>,
    pub byte_offset: usize,
    pub scanner_state: ScannerState,
}

pub struct Mouth {
    pub input: InputStack,
}
```

`Mouth::next_token()` receives the current Eqtb-backed catcode table and
interner. File input is a character source; macro replacement and inserted
tokens are token-list cursors.

`\makeatletter` and `\makeatother` cease to be lexer-known command names. Their
macro/compat execution changes catcode state through the assignment API.

### Token Provenance

Tokens carry origin rather than an unqualified byte span:

```rust
pub enum TokenOrigin {
    Source {
        file: FileId,
        revision: RevisionId,
        span: ByteSpan,
    },
    Expanded {
        expansion: ExpansionId,
        source_token: TokenId,
    },
    Generated {
        stable_id: GeneratedTokenId,
        reason: GeneratedTokenReason,
    },
}
```

Expansion stacks are interned in an arena:

```rust
pub struct ExpansionRecord {
    pub context: IdentityContextId,
    pub parent: ExpansionParent, // explicit execution root or expansion
    pub command: ScopedControlSequenceId,
    pub call_site: LexicalOrigin,
    pub definition_site: ExpansionDefinitionOrigin,
    pub argument_sources: Vec<LexicalOrigin>,
}
```

This avoids copying a full expansion stack onto every token while preserving
actual invocation and definition provenance.

Tests:

- runtime catcode changes affect only unread characters;
- `\input` file identity and byte cursor survive;
- macro token lists do not get retokenized under later catcodes;
- generated and expanded tokens resolve to the expected source stack;
- restore resumes at the exact source and token-list cursor;
- endline and comment scanner state round-trip.

Exit criteria:

- eager lexing is no longer used by `run_plain()` or production execution;
- `lex_plain()` remains only as a focused lexer helper/test API if still useful;
- the source registry binds every file/revision handle to the exact immutable
  UTF-8 bytes indexed by spans, with tested fresh/continuation/mounted/restore/
  rebase behavior;
- all source tokens carry complete lexical source identity;
- expansion provenance and scoped command identity come from the same validated
  execution context;
- legacy path-only and identity-complete snapshot capabilities are explicit,
  and old snapshots never receive fabricated identity;
- the private `ExecutedSourceSlice` constructor passes positive and rejection
  tests for context, revision, span, command, expansion, restore, rebase, and
  scanner-origin boundaries;
- schema v6 and its current path/span/textual provenance remain stable, while
  schema-v5 compatibility fixtures remain readable.

## V5: Macro, Prefix, And Command Model

### Macro Definition

Store full parameter text:

```rust
bitflags::bitflags! {
    pub struct MacroFlags: u8 {
        const LONG = 1 << 0;
        const OUTER = 1 << 1;
        const PROTECTED = 1 << 2;
    }
}

pub struct MacroDefinition {
    pub flags: MacroFlags,
    pub parameter_text: Arc<[ParameterToken]>,
    pub replacement: TokenListId,
    pub definition_source: SourceProvenance,
}

pub enum ParameterToken {
    Match(TokenKey),
    Argument(u8),
    EndMatch,
}
```

The argument scanner handles undelimited and delimited arguments, balanced
groups, runaway arguments, paragraph tokens, and source provenance.

### Prefix State

Collect prefixes before dispatch:

```rust
pub struct PrefixState {
    pub global: bool,
    pub long: bool,
    pub outer: bool,
    pub protected: bool,
}
```

The same path handles:

- `\global\long\def`;
- `\protected\def`;
- globally prefixed register arithmetic;
- illegal prefixes with diagnostics.

### Command Kinds

Separate expansion from execution:

```rust
pub enum Command {
    Expandable(ExpandableCommand),
    Unexpandable(UnexpandableCommand),
}
```

Core TeX commands, semantic adapters, and compatibility commands have distinct
variants. Package shims do not become core primitives merely because they are
registered in the same engine.

Tests:

- delimited and nested arguments;
- long and non-long paragraph arguments;
- protected macro in `\edef` and write expansion;
- forbidden outer tokens;
- `\noexpand`, `\unexpanded`, and `\expanded`;
- prefixed local/global definitions and arithmetic;
- malformed parameter text and runaway argument diagnostics.

Exit criteria:

- parameter count alone is no longer the macro grammar;
- `\long`, `\outer`, and `\protected` are not no-op execution branches;
- expandable and unexpandable dispatch rules are explicit;
- compatibility command growth does not enlarge the core command match
  indefinitely.

### Engine Runtime And Nest

Before VM-owned semantic events, centralize execution state and make current
mode explicit:

```rust
pub struct EngineState {
    pub eqtb: Eqtb,
    pub save_stack: SaveStack,
    pub input: InputStack,
    pub expansion: ExpansionState,
    pub conditionals: Vec<ConditionalFrame>,
    pub nest: Vec<NestFrame>,
    pub alignment: Option<AlignmentState>,
    pub page_builder: PageBuilderState,
    pub fonts: FontState,
    pub aux: AuxState,
    pub io: IoState,
    pub compat: CompatState,
}

pub enum NestFrame {
    Vertical(VerticalMode),
    InternalVertical(InternalVerticalMode),
    Horizontal(HorizontalMode),
    RestrictedHorizontal(RestrictedHorizontalMode),
    Math(MathModeState),
}
```

This first slice need not implement the full page builder, but it must make
paragraph start/end, vertical versus horizontal command behavior, inline versus
display math entry, grouping, and alignment nesting explicit enough for
SemanticSink decisions.

Exit criteria:

- text, space, paragraph, and math-boundary commands observe explicit mode;
- invalid mode transitions produce structured diagnostics;
- group exit and input exit cannot silently discard an active nest frame;
- Snapshot v2 has one state owner to serialize instead of a collection of
  unrelated VM fields.

## V6: VM-Owned SemanticSink

V6 consumes the validated V4 execution identity and `ExecutedSourceSlice` one
recovery family at a time. Public `StableEventId`, revision-aware provenance,
and semantic dependency relations belong to the separate readers-first event
schema stream after V4. They are not prerequisites for an internal bounded
consumer unless a family-specific test proves otherwise, and V6 does not assume
singular revision/dependency fields belong directly on `EventMeta`.

Before a family migrates, inventory whether its legacy scanner behavior is
post-success, dispatch-observed, failure-bound, or purely static. Prove with the
scanner's event output disabled that it does not alter VM state or control flow.
Commands that do not reach successful execution require a separately reviewed,
truthfully named bounded dispatch/failure input rather than a weakened
`ExecutedSourceSlice`.

Migrate event families in vertical slices:

1. text, space, and paragraph boundaries;
2. inline/display math boundaries;
3. headings and title metadata;
4. citations, references, labels, and links;
5. environments, lists, and footnotes;
6. floats, captions, and graphics;
7. tables and alignment structures.

Each slice follows this sequence:

1. add a divergence test for conditionals and macro generation;
2. characterize scanner side effects, failure paths, and legacy-checkpoint
   behavior for that family;
3. emit the event from actual command execution or its reviewed bounded input;
4. attach actual token/expansion provenance and shadow-consume the validated
   slice;
5. compare VM state, diagnostics, and event streams with the legacy scanner
   path;
6. switch production IR to the bounded path behind a family rollback switch;
7. remove or demote the corresponding whole-source scanner rule only after
   divergence and fallback reach zero.

The sink interface supports transactions before checkpoint replay is enabled:

```rust
pub trait SemanticSink {
    fn mark(&self) -> SinkMark;
    fn rollback(&mut self, mark: SinkMark);
    fn commit(&mut self, mark: SinkMark);
    fn emit(&mut self, event: RenderEventEnvelope);
}
```

The trace and diagnostic sinks use corresponding cursors so a cancelled or
rewound replay cannot duplicate output.

Exit criteria per event family:

- skipped conditionals emit nothing;
- macro-generated content emits the expected semantic event;
- local assignments affect event behavior only within their group;
- actual expansion frames produce provenance;
- replay does not duplicate events;
- scanner-enabled and scanner-disabled VM state/control flow remain equivalent;
- continuation, snapshot/restore, malformed/unsupported input, and the relevant
  missing-visible cases pass;
- the production IR path no longer consumes that family from the whole-source
  scanner.

Final V6 exit criteria:

- `capture_render_events_from_source()` is not called as an authoritative
  whole-document step in `run_plain()`;
- the source scanner exists only as bounded recovery/debug compatibility;
- every production event declares producer and confidence;
- every inventoried family has zero bounded-vs-scanner divergence and zero
  production scanner fallback through the required rollout window;
- legacy path-only checkpoints have an explicit rebuild, retirement, or legacy
  capability policy;
- sequence remains build-local and no public identity migration is implied by
  scanner retirement.

### Current V6 Implemented Vertical Slices (Phase Exit Open)

As of 2026-07-30, the footnote, standard/common-profile front-matter, direct
bibliography item, and bibliography materialization vertical slices are
complete.

Footnotes:

- `\footnote`, `\footnotemark`, `\footnotetext`, and `\tablefootnote` emit
  primitive or macro-produced events only when execution reaches them;
- note bodies collect executed text, inline semantic events, and math in one
  ordered transaction rather than rebuilding them from raw source;
- detached marks use a one-shot pending identity that survives continuation
  snapshots, changed-child replay, and cross-file mark/body boundaries;
- note IDs are remapped across pending, active, queued, and already committed
  events when scanner recovery IDs are atomically replaced;
- the current semantic snapshot schema validates that a pending mark refers to
  exactly one captured `FootnoteMark`;
- false-conditional, macro, alias, override, nested-mark, package-shim, and
  table-cell cases have focused tests.

Front matter and migrated profile metadata:

- `\title`, `\author`, `\date`, and `\maketitle` emit typed metadata and title
  flush events from VM execution;
- `\affil`, `\affiliation`, `\institute`, `\email`, `\keywords`, and `\pacs`
  use the same execution-owned metadata path;
- the mini-kernel and article, authblk, LLNCS, REVTeX, and WACV compatibility
  macros delegate to internal semantic primitives without changing legacy
  text output;
- ICML title, author, affiliation, correspondence, keywords, and title-block
  flush commands consume their real one- or two-argument signatures. The
  `icmlYYYY.sty` preview shim keeps package execution bounded, and affiliation
  labels do not leak into visible text;
- false conditionals emit nothing, aliases preserve primitive meaning, and
  user redefinitions suppress matching scanner recovery;
- author arguments expand user macros while preserving top-level `\and`, `\\`,
  and `\thanks` semantics, including separators introduced by expansion;
- primitive events carry argument-content and invocation spans, while
  macro-produced events carry the actual expansion stack;
- reconciliation retains compatible scanner event IDs but replaces selected
  recovery events with high-confidence primitive or macro events;
- the current semantic snapshot preserves selected scanner IDs and pending
  executed front-matter events across continuation replay;
- compact event-to-IR-to-display-list tests cover generic, class-shim, and ICML
  metadata.

Direct bibliography items:

- `thebibliography` execution owns bibliography depth, and `\bibitem` emits a
  structural item only when execution reaches it inside that environment;
- the mini-kernel delegates `\bibitem` to the internal `\latexdbibitem`
  primitive instead of defining a blank compatibility macro;
- item capture retains executed visible text while rolling back nested
  footnote, graphic, heading, list, table, and caption events so they do not
  leak into the surrounding event stream;
- false conditionals emit nothing, aliases preserve primitive meaning, user
  redefinitions suppress matching scanner recovery, and misplaced items leave
  following body text visible;
- primitive events preserve exact invocation and citation-key spans, while
  macro-produced items carry their actual expansion frames;
- reconciliation retains compatible scanner event IDs but promotes only
  actually executed items to high-confidence primitive or macro events;
- the current semantic snapshot preserves bibliography depth, active item
  captures, nested semantic baselines, and the scoped forced-text recovery
  range across input continuation replay.

Bibliography materialization:

- `\bibliography{...}` and optioned `\printbibliography[...]` consume their
  arguments only when execution reaches them, then execute local
  `jobname.bbl` content through the normal input/event path;
- false conditionals, user overrides, and skipped dynamic occurrences do not
  read the `.bbl` or retain its scanner recovery events;
- macro-generated commands preserve call provenance and source-order event
  placement;
- continuation snapshots preserve the entry jobname, external input
  dependency, occurrence authority, and event stream equality after replay.

Non-visible bibliography metadata:

- `\addbibresource`, `\bibliographystyle`, `\nocite`, and `\defcitealias`
  consume their real optional and mandatory arguments only when VM execution
  reaches them;
- direct calls, macro expansion, `\let` aliases, and continuation replay keep
  resource paths, options, styles, keys, and alias text out of legacy output,
  RenderEvent text, SemanticDocumentIr, and PageDisplayList text;
- user definitions continue to take precedence and preserve their visible
  execution semantics;
- executed source ranges suppress matching scanner recovery, including
  aliased invocations whose source command name is not one of the four
  built-ins;
- this slice establishes execution and visibility ownership only. Resource
  registration, style selection, `nocite` inclusion, and citation-alias
  definitions are not yet first-class semantic aux records.

Bibliography punctuation and delimiters:

- `\addcomma`, `\addcolon`, `\addsemicolon`, `\adddot`, `\adddotspace`,
  `\isdot`, bibliography dash/slash helpers, and
  `\bibopen...`/`\bibclose...` delimiters are typed VM compatibility
  primitives;
- helper output flows through the normal executed text/space capture path, so
  false conditionals emit nothing, macro and `\let` expansion execute the
  helper, and user definitions continue to win;
- canonical command names survive continuation snapshots, while direct and
  macro-generated bibliography item text reaches SemanticDocumentIr and
  PageDisplayList unchanged;
- visible helper execution does not use non-visible source suppression.
  Executed text replaces compatible scanner payloads, while bounded recovery
  normalization remains available for unsupported package output;
- bibliography item projection protects literal square and curly delimiters
  after execution, rather than reinterpreting visible braces as TeX grouping.

Explicit bibliography spacing:

- `\addspace`, `\addabbrvspace`, `\addnbspace`, `\addthinspace`,
  `\addlowpenspace`, and `\addhighpenspace` emit executed interword spaces
  through the same typed bibliography text primitive;
- macro expansion, `\let` aliases, false conditionals, user overrides, and
  continuation replay follow normal VM execution semantics;
- helper metadata records whether following source whitespace attaches to the
  emitted punctuation. Range dashes, slashes, and opening delimiters suppress
  that gap, while periods, em dashes, and closing delimiters preserve a
  whitespace-only source gap;
- the source text needed for this compatibility policy is available during
  both event-capture and legacy-only runs, then discarded after execution, so
  dual-write legacy output does not depend on capture mode;
- macro-generated spacing reaches SemanticDocumentIr and PageDisplayList
  without relying on whole-source scanner expansion.

Bibliography state-helper visibility:

- `\newunit` emits the compatibility separator, while `\finentry`, `\unspace`,
  and `\urlprefix` execute without visible text;
- `\nopunct` emits no command text but preserves a following whitespace-only
  source gap, matching the current compatibility projection;
- direct calls, macro expansion, `\let` aliases, false conditionals, user
  overrides, continuation replay, SemanticDocumentIr, and PageDisplayList are
  covered;
- this is a visibility migration, not a complete biblatex punctuation-state
  implementation. Pending punctuation, look-ahead suppression, `\unspace`
  rollback, and package-defined tracker interactions remain future work.

Bibliography one-argument wrappers:

- `\mkbibquote`, `\mkbibparens`, `\mkbibbrackets`, `\mkbibbraces`,
  common style/name wrappers, `\mkbibsuperscript`, `\mkbibsubscript`,
  `\enquote`, and `\parentext` are typed VM compatibility primitives;
- execution consumes an optional star and replays the visible argument through
  the normal token queue, so nested wrappers and macro-generated content retain
  execution order rather than being flattened by source recovery;
- transparent wrappers preserve readable text, quote/delimiter wrappers add
  their visible decoration, and super/subscript wrappers attach to the
  preceding text. Opening decoration suppresses an artificial separator before
  a nested wrapper;
- direct calls, macro expansion, `\let` aliases, false conditionals, user
  overrides, continuation replay, SemanticDocumentIr, and PageDisplayList are
  covered;
- this slice owns visible execution only. Font style, name-part semantics, and
  superscript/subscript layout are not yet represented as structured IR or
  layout nodes.

Bibliography string lookup:

- `\bibstring` is a typed VM primitive that consumes an optional star, fully
  expands its key argument, and executes the localized visible result through
  the normal token queue;
- the first lookup maps `andothers` to `et al`; unknown keys retain a readable
  normalized fallback, and an empty key emits no artificial separator;
- capture-disabled legacy output, macro-expanded keys, `\let` aliases, false
  conditionals, user overrides, continuation replay, SemanticDocumentIr, and
  PageDisplayList are covered;
- this is not a complete biblatex localization engine. Locale selection,
  plural forms, capitalization variants, and the full string table remain
  future work.

Bibliography field-wrapper visibility:

- `\bibinfo{field}{value}` and `\bibfield{field}{value}` are typed VM
  compatibility primitives. Execution consumes the field selector without
  expanding it and replays only the value token list through the normal queue;
- capture-disabled output, nested wrappers, macro expansion, `\let` aliases,
  false conditionals, user overrides, continuation replay,
  SemanticDocumentIr, and PageDisplayList are covered;
- the existing semantic aux scanner continues to extract author, title, year,
  identifier, and URL metadata for citation commands, and its regression tests
  remain green;
- this slice owns visible execution and selector suppression only. Replacing
  the independent aux field scan with VM-produced semantic metadata records is
  still future work.

Bibliography identifier visibility:

- `\doi{value}` and `\eprint{value}` use the typed transparent bibliography
  wrapper primitive, so their value token lists execute without leaking raw
  command names;
- capture-disabled output, nested wrappers, macro expansion, `\let` aliases,
  false conditionals, user overrides, continuation replay,
  SemanticDocumentIr, and PageDisplayList are covered;
- existing DOI/eprint citation-field aux regression remains green;
- this slice preserves visible values only. It does not yet emit identifier
  semantic nodes, construct DOI/arXiv targets, or create link annotations.

Natbib year-suffix visibility:

- `\natexlab{value}` and `\NAT@exlab{value}` use an attached transparent VM
  wrapper, preserving the suffix value without leaking command markup;
- when `@` has its normal non-letter catcode, raw `.bbl` input tokenizes the
  latter spelling as `\NAT` followed by `@exlab`. A bounded compatibility
  primitive recognizes exactly that suffix, restores all probed tokens on a
  mismatch, and otherwise replays the visible argument through the normal
  token queue;
- capture-disabled output, macro expansion, `\let` aliases tokenized under
  `\makeatletter`, false conditionals, user overrides, continuation replay,
  SemanticDocumentIr, PageDisplayList, and the internal compiler path are
  covered;
- stored `.bbl` sources remain byte-for-byte input artifacts for replay and
  debugging. Only executed output and semantic aux projections are normalized,
  so source preservation is not confused with rendered-text fidelity.

Phantom wrapper visibility:

- `\phantom{...}`, `\hphantom{...}`, and `\vphantom{...}` are typed VM
  primitives that consume one hidden argument without leaking the command or
  its text into legacy output;
- the executed invocation records a suppression range, including the macro
  call range when expanded, so scanner-recovery citations, references, links,
  labels, and math nested inside hidden content cannot survive reconciliation;
- canonical primitive names preserve `\let` aliases across continuation
  snapshots. Capture on/off, false conditionals, user overrides, input-boundary
  replay, mini-kernel base snapshots, mounted `.bbl` input,
  SemanticDocumentIr, PageDisplayList, and focused compiler smoke are covered;
- this is a compatibility visibility slice, not TeX box semantics. The VM does
  not yet construct invisible boxes with retained width, height, or depth, and
  it does not execute hidden-argument side effects. Those behaviors require the
  hlist/vlist Layout IR and a scoped hidden-box execution model.

Bibliography case-wrapper visibility:

- `\NoCaseChange{...}`, `\MakeSentenceCase{...}`, and
  `\MakeTitleCase{...}` use the transparent VM bibliography-wrapper primitive,
  and the latter two accept their optional starred spelling without leaking
  command markup;
- visible argument tokens return to the normal execution queue. Macro
  expansion, `\let` aliases, false conditionals, user overrides, and
  input-boundary continuation replay therefore follow VM execution rather than
  whole-source scanner guesses;
- the wrapper adds no synthetic separator, so adjacent forms such as
  `Mc\NoCaseChange{Donald}` remain `McDonald`. Source whitespace remains the
  only interword separator;
- capture-disabled output, mounted raw `.bbl` input, SemanticDocumentIr,
  PageDisplayList, and focused compiler smoke are covered. Stored `.bbl`
  sources remain byte-for-byte input artifacts;
- this slice preserves the visible argument unchanged. It does not implement
  sentence-case or title-case transformation, locale rules, protected
  capitalization regions, or biblatex's complete case-conversion semantics.

No-output state-helper execution:

- `\leavevmode` and `\unskip` are typed VM primitives, so direct calls, macro
  expansion, `\let` aliases, false conditionals, user overrides, and
  input-boundary continuation replay follow execution rather than scanner
  normalization;
- `\leavevmode` currently produces no visible output. It does not yet create or
  transition a TeX horizontal-mode nest;
- `\unskip` removes trailing whitespace from the current legacy linear output,
  removes the immediately preceding executed `Space` event, and trims the
  current structured-table cell before following punctuation is captured;
- capture-disabled output, mounted raw `.bbl` input, SemanticDocumentIr,
  PageDisplayList, structured-table cells, and focused compiler smoke are
  covered. Stored `.bbl` sources remain unchanged;
- this is a compatibility execution slice, not hlist glue semantics. It cannot
  remove glue from an output prefix already externalized before a continuation
  checkpoint. True mode transitions, list-tail inspection, and cross-boundary
  glue mutation belong to the future nest and LayoutIr implementation.

Bibliography box-wrapper execution:

- `\framebox`, `\makebox`, `\raisebox`, and `\parbox` use a typed command enum
  rather than one generic wrapper signature;
- `framebox/makebox` consume up to two bracket options or picture-mode
  `(width,height)[position]`, `raisebox` consumes lift plus optional height and
  depth, and `parbox` consumes up to three positioning/height options plus
  width. Only the visible body returns to the normal execution queue;
- the mini-kernel no longer defines lossy `makebox`, `raisebox`, or `parbox`
  macros that shadow these primitives. Document and package definitions still
  override builtins through the normal Eqtb path;
- capture-disabled output, mounted raw `.bbl` input, macro expansion, `\let`
  aliases, false conditionals, user overrides, input-boundary continuation
  replay, SemanticDocumentIr, PageDisplayList, and focused compiler smoke are
  covered;
- this slice preserves visibility and signatures only. It does not evaluate
  dimensions or create positioned, framed, raised, paragraph, or picture
  boxes. Those geometry and alignment semantics require typed dimensions and
  hlist/vlist LayoutIr nodes.

Visible text-symbol execution:

- `\textquotesingle`, `\textquotedbl`, `\textless`, `\textgreater`,
  `\textbar`, and `\slash` use a typed command carrying the canonical snapshot
  name and visible character;
- direct execution writes the character through the normal executed-text and
  legacy-output paths. It does not restore the source whitespace consumed after
  a control word, so `Quote\textquotesingle s` remains `Quote's`;
- capture-disabled output, mounted raw `.bbl` input, macro expansion, `\let`
  aliases, false conditionals, user overrides, input-boundary continuation
  replay, SemanticDocumentIr, PageDisplayList, and focused compiler smoke are
  covered;
- this slice defines visible character semantics only. Font encoding, glyph
  selection, typography, and renderer-neutral symbol runs remain font/LayoutIr
  work.

Text-script wrapper execution:

- `\textsuperscript{...}` and `\textsubscript{...}` use a typed VM command
  instead of mini-kernel macros. The visible argument returns to the normal
  token queue, so macro expansion, `\let` aliases, false conditionals, and user
  overrides follow execution semantics;
- wrapper depth is explicit continuation state. Only the outermost closing
  boundary arms the compatibility word separator, entering an adjacent script
  clears that pending separator, and punctuation consumes it without adding a
  gap. This preserves `Edition2a.` and `Nestedabc.` while separating
  `Marker1 Word`;
- the pending separator is emitted through both legacy output and executed
  `Space` capture, keeping SemanticDocumentIr and PageDisplayList text
  consistent. The wrapper depth and pending state survive a checkpoint taken
  inside a nested script body;
- the mini-kernel no longer shadows either command. Document, class, and
  package definitions still override the builtins through the normal Eqtb
  path;
- capture-disabled output, mounted raw `.bbl` input, macro expansion, aliases,
  conditionals, overrides, active-wrapper input-boundary replay,
  SemanticDocumentIr, PageDisplayList, focused compiler smoke, and the
  exact-output composite wrapper smoke are covered;
- this slice owns linear visible-text attachment only. It does not create
  superscript/subscript semantic nodes, change font size, shift baselines, or
  shape glyphs. The compatibility boundary currently recognizes ASCII
  alphanumeric word starts; structured script typography and broader text
  segmentation belong to Math/LayoutIr.

This does not satisfy the final V6 exit criteria. `run_plain()` still invokes
the whole-source scanner before execution, stable cross-revision identity does
not yet exist while event sequence remains explicitly build-local, and full
bibliography localization, package-specific multi-argument/style wrappers,
full punctuation-state fidelity, unbridged profile commands, plus parts of
math, table, and wrapper recovery remain. Subsequent slices must migrate those
remaining families and narrow the scanner entry point to source regions that
execution explicitly delegates for recovery.

## Shared Diagnostic Contract

Develop the shared schema in its dependency-neutral architecture stream and
migrate command-specific diagnostics with their owning V3-V8 batches.

As bounded compatibility bridges, the internal compiler projects
`missing graphic asset ...` render events and deterministic unconvertible-EPS
materialization state into deduplicated HMR warnings, preserving primary-file
provenance (`ab589f7`, `71f276b`). The EPS adapter uses the renderer's existing
`Eps`/no-PDF-form/no-raster-fallback state; it does not duplicate PDF or raster
decode policy. These bridges fix daemon/viewer loss without changing a wire
type, but they are not the shared contract: VM diagnostics, render-event
messages, image annotations, renderer outcomes, HMR diagnostics, and WASM
strings still have different fields, and phase/recovery cannot be reconstructed
reliably from their text.

The first versioned slice is implemented in render-event schema v6 (`dcb4d08`,
`2f3eee8`). `RenderDiagnosticCode` names missing package, class, input, cyclic
input, and graphic asset events. Only the missing-graphic code carries the
narrow `asset_ref` required to join display-list image operations; the model
accessor accepts only that complete code/value pair. The VM keeps owning the
existing human-readable message and `EventOrigin` keeps owning
producer/confidence. Both compiler adapters consume the typed accessor, so a
reworded message remains display content rather than a hidden protocol, and a
prefix-looking mismatched payload fails closed.

Schema-v5 message-only diagnostics retain structural read/display/exact
round-trip compatibility as `Unknown` with no asset reference. They are not
reclassified from English text and therefore do not recreate typed
missing-graphic projections. Repository tracing found production artifact
writes and serving for `events.json`, but no compiler path that replays a
persisted v5 stream through those adapters; current deserialization sites are
artifact assertions. Publishing schema-v6 artifacts is nevertheless the
rollback boundary: rollback code must retain v6 read support. Severity, phase,
recovery, related diagnostics, renderer outcomes, and versioned VM/HMR/WASM
adapters remain outside this slice, so the shared contract is still partial.

Use one structured diagnostic shape from mouth through rendering:

```rust
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: Severity,
    pub message: String,
    pub primary: Option<SourceProvenance>,
    pub related: Vec<RelatedDiagnostic>,
    pub recovery: RecoveryAction,
    pub phase: DiagnosticPhase,
}
```

Initial phases:

- mouth;
- expansion;
- execution;
- semantic lowering;
- checkpoint/replay;
- layout;
- rendering.

Initial VM codes include undefined command, runaway argument, bad parameter
text, forbidden outer token, group/conditional mismatch, missing input, invalid
assignment, unsafe checkpoint, and lossy recovery.

Diagnostics are data, not formatted strings. Human-readable transcript text is
derived at CLI/UI boundaries.

## V7: Snapshot V2 And Transactional Replay

Use two snapshot contracts.

### FormatSnapshot

Created only at a verified safe format/preamble boundary:

```rust
pub struct FormatSnapshot {
    pub schema: SnapshotSchemaVersion,
    pub engine_profile: EngineProfileId,
    pub eqtb_root: EqtbRootId,
    pub token_arena_root: TokenArenaRootId,
    pub fonts: FontStateSnapshot,
    pub page_parameters: PageParameterSnapshot,
    pub compat: CompatStateSnapshot,
    pub semantic_hash: StateHash,
}
```

Safe format boundaries have no active argument scan, open conditional/group,
partial input command, or page contribution.

### ContinuationCheckpoint

Stores complete execution continuation:

```rust
pub struct ContinuationCheckpoint {
    pub schema: SnapshotSchemaVersion,
    pub engine_profile: EngineProfileId,
    pub eqtb_root: EqtbRootId,
    pub save_stack: SaveStackSnapshot,
    pub input_stack: InputStackSnapshot,
    pub conditionals: Arc<[ConditionalFrame]>,
    pub expansion: ExpansionStackSnapshot,
    pub nest: NestSnapshot,
    pub alignment: Option<AlignmentSnapshot>,
    pub page_builder: PageBuilderSnapshot,
    pub fonts: FontStateSnapshot,
    pub aux: AuxStateSnapshot,
    pub io: IoStateSnapshot,
    pub dependency_cursor: TraceCursor,
    pub event_cursor: EventCursor,
    pub diagnostic_cursor: DiagnosticCursor,
    pub next_stable_ids: StableIdSnapshot,
    pub deterministic_environment: DeterministicEnvironment,
}
```

Use copy-on-write or persistent roots for Eqtb, macro/token arenas, immutable
input buffers, font definitions, and semantic aux maps. A checkpoint retains
root IDs plus small continuation state instead of cloning all maps per page.

Replay equality compares:

- RenderEvent stream;
- semantic document IR;
- structured diagnostics;
- dependency trace;
- semantic aux;
- LayoutIr and PageDisplayList;
- output and write state.

Tail reuse requires equality of:

- VM semantic-state hash;
- page/display-list hash;
- dependency read set;
- aux state;
- write, mark, insertion, and float state;
- engine/profile/font/layout schema keys.

Exit criteria:

- full build equals restore-plus-replay at each supported boundary;
- event/trace/diagnostic transactions cannot duplicate output;
- incompatible snapshot schemas or engine profiles fail explicitly;
- checkpoint size and restore latency are reported;
- page equality alone never terminates replay.

## V8: SemanticDocumentIr V3

The current `DocumentIr` remains a semantic document model. During migration,
use a compatibility alias if renaming the public type would create unrelated
churn:

```rust
pub type DocumentIr = SemanticDocumentIr;
```

It is explicitly:

- not TeX VM state;
- not an hlist/vlist;
- not renderer-specific;
- derived and rebuildable from events plus aux/assets.

### Common Metadata

```rust
pub struct IrMeta {
    pub id: NodeId,
    pub source: SourceProvenance,
    pub originating_events: EventRange,
    pub semantic_hash: ContentHash,
    pub dependencies: SmallVec<[DependencyId; 4]>,
}
```

All block and inline nodes carry `IrMeta`.

### Structured Inline Content

Heading, caption, bibliography, table-cell, and link display content use one
`InlineContent = Vec<InlineNode>` representation rather than flattening to
strings.

Inline and display math use the same `MathFragment`; only mode and display
metadata differ. The fragment receives `MathList` from VM execution and never
invokes a raw-source parser.

Dimensions preserve raw tokens and typed interpretation:

```rust
pub struct DimensionExpression {
    pub parsed: Option<DimensionAst>,
    pub evaluated: Option<Scaled>,
    pub raw_tokens: TokenListId,
}
```

### Builder Frame Stack

Replace independent active-state fields with one frame stack for paragraphs,
environments, layout containers, floats, captions, lists, footnotes, tables,
rows, and cells.

Builder output includes:

```rust
pub struct DocumentBuildOutput {
    pub document: SemanticDocumentIr,
    pub diagnostics: Vec<Diagnostic>,
    pub event_to_node: BTreeMap<StableEventId, NodeId>,
}
```

Mismatched begin/end sequences emit explicit unexpected-end,
implicitly-closed, unclosed-at-EOF, and mode-mismatch diagnostics.

Exit criteria:

- nested inline semantics survive headings, captions, bibliography, and cells;
- every node has stable identity, semantic hash, dependency, and event mapping;
- builder recovery is deterministic and diagnosed;
- the IR can be reconstructed from a replayed event stream.

## LayoutIr Boundary

Add a renderer-neutral TeX layout layer between semantic IR and display lists:
the existing `DocumentLayout` string/line pagination helper is not this
boundary and is not renamed to imply hlist/vlist semantics.

```rust
pub enum LayoutNode {
    Glyph(GlyphNode),
    HBox(HBox),
    VBox(VBox),
    Rule(RuleNode),
    Glue(GlueNode),
    Kern(KernNode),
    Penalty(PenaltyNode),
    Discretionary(DiscretionaryNode),
    Insert(InsertNode),
    Mark(MarkNode),
    Adjustment(AdjustmentNode),
    LinkStart(LinkTarget),
    LinkEnd,
    Image(ImageNode),
}
```

Layout values use scaled integers and explicit glue-set state. Page coordinates
are introduced only when lowering a completed page into `PageDisplayList`.

The separation is:

```text
RenderEvent
  -> SemanticDocumentIr
  -> semantic lowering
  -> paragraph/table/MathList structures
  -> LayoutIr hlist/vlist
  -> line breaker and page builder
  -> PageDisplayList
```

Semantic headings, citations, and figures do not disappear into LayoutIr;
source/node identity maps survive lowering.

## Direct Commit Batches

The direct implementation sequence is:

1. `test: characterize VM semantic divergence`
2. `fix(checkpoint): reject reuse when continuation state is not proven safe`
3. `refactor(tex-vm): split engine modules without behavior changes`
4. `refactor(events): quarantine scanner recovery and expose event sequence`
5. characterize scanner special events, introduce the typed `EventOrigin`
   write boundary, classify all remaining compatibility constructors, guard
   production writes statically, and migrate one event family or fixture class
   per green commit; remove both raw constructors together after the inventory
   reaches zero incidental fixtures (`0940368`)
6. centralize compatible location-only overlap (seven families in `decccd7`)
   and source-only insertion (four families in `694a0ee`), then audit
   bibliography/graphic identity and sequence reuse; defer concrete bounded
   `ExecutedSourceSlice` construction until step 11 supplies its identity
   lifecycles
7. close the footnote sequence-as-identity regression (`ba9424d`) and finish
   the V2 all-family suppression/expected-failure audit
8. prove V3 independence with differential behavior, snapshot compatibility,
   changed-path, and added-symbol guards before moving production ownership
   (`2289907`, `fe6b4df`, `d9cdf02` landed)
9. `feat(tex-vm): introduce Eqtb and SaveStack for one bounded ownership class`
10. migrate remaining dimen/skip/toks/catcode/mathcode/font assignment classes in bounded
   green commits
11. approve the V4 identity ADR, then add source registry shadow mode, lexical
    origins, expansion/scoped-command identity, readers-first snapshot
    capability, and the validated internal `ExecutedSourceSlice` in that order
12. `feat(tex-vm): execute through a streaming mouth`
13. `feat(tex-vm): support parameter text and command prefixes`
14. `refactor(tex-vm): centralize execution mode and nest state`
15. migrate V6 recovery/SemanticSink families one vertical slice per green
    commit, then retire the scanner only after the legacy policy and zero-
    divergence/fallback gates pass
16. `feat(checkpoint): add transactional continuation snapshots`
17. `refactor(ir): introduce SemanticDocumentIr metadata and frame builder`
18. `feat(layout): introduce renderer-neutral LayoutIr`

The public event identity/schema and shared diagnostic contract are separate
reviewed streams. The former starts only after V4 defines provenance/dependency
semantics and readers are available; the latter first chooses a
dependency-neutral canonical owner and versioned VM/event/HMR/WASM/snapshot
adapters. Neither is inserted into the V2/V3/V4 critical path.

These are commit-sized batches, not review phases. If a batch is still too
large, split it by assignment class or event family without reordering the
dependency chain.

## Test Matrix

### Default CI

- focused unit tests for each engine module;
- divergence and local/global assignment fixtures;
- mouth/catcode/input provenance fixtures;
- macro parameter/prefix expansion fixtures;
- event producer/confidence/sequence goldens, followed by stable-ID goldens
  after token-origin migration;
- event-to-IR mapping and builder recovery goldens;
- format and continuation snapshot equivalence;
- selected end-to-end internal render fixtures.

### Push CI

- broader VM, IR, checkpoint, and internal compiler tests;
- macro-generated/conditional/package interaction fixtures;
- native/WASI event/IR/display-list parity where applicable;
- browser actual-page artifact tests.

### Nightly/Manual

- full arXiv smoke and licensed corpora;
- randomized group/assignment/macro grammars;
- checkpoint at every declared safe boundary;
- replay/cancel/restart stress;
- snapshot size, restore latency, and event duplication metrics.

## Migration Observability

Build metadata reports:

- authoritative event producer counts;
- scanner-recovery event count and affected source spans;
- event stable-ID reuse and churn;
- Eqtb root and save-stack depth;
- input and expansion stack depth;
- snapshot schema, type, size, and restore status;
- replayed tokens/files and sink rollback counts;
- semantic IR node reuse/churn;
- LayoutIr and page reuse counts.

The development UI must distinguish:

- VM-executed semantic output;
- scanner-recovered output;
- external final TeX output;
- missing or lossy compatibility behavior.

## File Ownership Map

| Area | Initial files |
| --- | --- |
| VM facade and loop | `tex-vm/src/lib.rs`, `engine.rs`, `command.rs` |
| Input and mouth | `tex-vm/src/input.rs`, `mouth.rs`, `tex-lexer` |
| Expansion and macros | `tex-vm/src/expansion.rs`, `macro_def.rs` |
| State and assignment | `tex-vm/src/eqtb.rs`, `save_stack.rs`, `assignment.rs`, `registers.rs` |
| Diagnostics and sinks | `tex-vm/src/diagnostic.rs`, `semantic_sink.rs` |
| Compatibility | `tex-vm/src/compat/` |
| Recovery scanner | `tex-vm/src/source_recovery.rs`; split a crate only after the boundary stabilizes |
| Token provenance | `tex-tokens` |
| Event contract | `tex-render-model/src/events.rs` |
| Semantic IR | `tex-render-model/src/ir.rs` |
| IR builder | `tex-layout/src/document_ir_builder.rs` |
| Layout IR | `tex-layout/src/layout_ir.rs` initially |
| Checkpoints | `tex-vm/src/snapshot.rs`, `tex-checkpoint` |

Two workers must not edit the same module family concurrently. Mechanical split
ownership and semantic migration ownership are never active on the same files
at the same time.

## Relationship To Math And Browser Plans

[`math-rendering-plan.md`](./math-rendering-plan.md) depends on this plan:

- browser PDF/page delivery can proceed independently;
- bundled font resolution can proceed independently;
- VM-owned `MathList` begins only after Eqtb, streaming Mouth, macro parameter
  text, and SemanticSink foundations are active;
- persistent browser `CompilerSession` begins only after continuation snapshot
  equivalence exists;
- math and page cache keys consume stable event/node IDs rather than sequence or
  raw byte offsets.

The math plan does not reopen a raw-source parser as a shortcut around this
dependency.

## Explicit Non-Solutions

The following are prohibited as architectural progress:

- adding more whole-source scanner patterns to production semantics;
- keeping eager whole-document tokenization while emulating runtime catcodes;
- fixing register locality separately in each register map;
- treating `\long`, `\outer`, or `\protected` as no-op compatibility commands;
- assigning high confidence to scanner-derived events;
- using event sequence as stable identity;
- snapshotting only compatibility maps while omitting input/conditional/nest
  continuation;
- replaying events without sink transactions;
- flattening nested semantic inline content to strings;
- merging semantic document IR with hlist/vlist LayoutIr;
- starting MathList migration before execution-generated events are
  authoritative.
