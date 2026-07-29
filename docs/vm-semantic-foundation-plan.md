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
- `tex-vm/src/lib.rs` is 47,997 lines in the reviewed checkout, including core
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

Use explicit event producers:

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

`RenderEventEnvelope::new()` must not silently assign high confidence to
events whose origin is unknown. Use explicit constructors or a required
`EventMeta`:

```rust
RenderEventEnvelope::from_primitive(...)
RenderEventEnvelope::from_macro(...)
RenderEventEnvelope::from_compat_command(...)
RenderEventEnvelope::from_scanner_recovery(...)
RenderEventEnvelope::fallback(...)
```

Policy:

- VM primitive or verified macro path: high confidence;
- compatibility command with defined semantics: high or medium, declared by
  the adapter;
- scanner recovery: medium or low;
- lossy fallback: fallback confidence;
- a no-op shim emits a diagnostic and is not counted as supported behavior.

### Recovery Scope

The final recovery scanner must not scan the whole file and feed a second
production event stream into IR.

Allowed recovery input:

```rust
pub struct ExecutedSourceSlice {
    pub file: FileId,
    pub revision: RevisionId,
    pub span: ByteSpan,
    pub command: Option<ControlSequenceId>,
    pub expansion: Option<ExpansionId>,
}
```

The VM creates this slice only after reaching the construct through normal
execution. The scanner may recover a bounded command, argument, environment,
or math region from that slice.

During V2-V6 migration, the old whole-source scanner may temporarily remain:

- in debug artifacts;
- in migration differential tests;
- as an explicitly low-confidence compatibility source for event families that
  have not migrated yet.

No new production feature is added to that whole-source path. Each V6 vertical
slice removes one event family from the legacy bridge. The bounded
`ExecutedSourceSlice` interface is the only remaining production recovery path
at final V6 exit. Known false-conditional leakage remains an explicit failing
characterization until the corresponding family migrates; it must not be
hidden by a high-confidence event.

### Event Sequence

At this stage, rename the current meaning honestly:

```rust
pub struct EventMeta {
    pub sequence: u64,
    pub source: SourceProvenance,
    pub mode: SemanticMode,
    pub producer: EventProducer,
    pub confidence: SemanticConfidence,
    pub revision: RevisionId,
    pub dependencies: SmallVec<[DependencyId; 4]>,
}
```

`sequence` is build-local ordering. Do not derive `StableEventId` yet: it
cannot be defined correctly until V4 supplies file-aware token origins and
interned expansion records. Footnote or node identity must stop depending on
the next event sequence before replay reuse is enabled.

Exit criteria:

- scanner events are visibly distinguishable in JSON/debug artifacts;
- no default constructor overstates producer or confidence;
- known false-conditional leakage is exposed as a low-confidence legacy
  recovery failure until its event family migrates;
- the current `event_id` contract is migrated/versioned as build-local
  `sequence`;
- no code treats sequence as a revision-stable identity.

## V3: Eqtb And SaveStack

Replace split control-sequence scopes and independent register maps with one
assignment model.

```rust
pub enum EqKey {
    ControlSequence(ControlSequenceId),
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

All assignment commands pass through one API:

```rust
pub fn assign(
    &mut self,
    key: EqKey,
    value: EqValue,
    requested_scope: AssignmentScope,
) -> Result<(), Diagnostic>;
```

The API resolves `\global` and `\globaldefs`, saves the prior local value once
per group level, and applies TeX-compatible global restore suppression.

Migration order:

1. control-sequence definitions and `\let`;
2. count registers and arithmetic;
3. dimen registers;
4. skip and muskip registers;
5. token registers;
6. catcodes;
7. mathcodes and delcodes;
8. fonts, boxes, and remaining parameters.

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

## V4: Streaming Mouth And Token Origins

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
    pub parent: Option<ExpansionId>,
    pub command: ControlSequenceId,
    pub call_site: TokenOriginId,
    pub definition_site: Option<SourceSpanRef>,
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
- all source tokens identify file and revision;
- expansion provenance comes from the VM expansion arena.

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

Before migrating the first production family, add `StableEventId` using the
file-aware token and expansion origins from V4:

```rust
pub struct EventMeta {
    pub sequence: u64,
    pub stable_id: StableEventId,
    // producer, confidence, source, revision, dependencies...
}
```

The stable anchor combines stable file identity, a source token anchor or
bounded token fingerprint, expansion call-chain identity, semantic role, and a
local ordinal among equivalent siblings. Absolute byte offsets and sequence
numbers alone are not stable identities. Multi-revision tests cover reuse,
collision detection, and deterministic disambiguation.

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
2. emit the event from actual command execution;
3. attach actual token/expansion provenance;
4. compare the VM stream with the legacy scanner stream in a debug test;
5. switch production IR to the VM event for that capability;
6. remove or demote the corresponding whole-source scanner rule.

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
- the production IR path no longer consumes that family from the whole-source
  scanner.

Final V6 exit criteria:

- `capture_render_events_from_source()` is not called as an authoritative
  whole-document step in `run_plain()`;
- the source scanner exists only as bounded recovery/debug compatibility;
- every production event declares producer and confidence;
- sequence and stable identity are separate, and replay preserves next-ID
  state.

### Current V6 Implementation Status

As of 2026-07-29, the footnote, standard/common-profile front-matter, direct
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

This does not satisfy the final V6 exit criteria. `run_plain()` still invokes
the whole-source scanner before execution, event sequence is not yet separated
from stable cross-revision identity, and package-specific bibliography helpers,
unbridged profile commands, plus parts of math, table, and wrapper recovery
remain scanner-only. Subsequent slices must migrate those remaining families
and narrow the scanner entry point to source regions that execution explicitly
delegates for recovery.

## Shared Diagnostic Contract

Introduce the shared schema in V2 and migrate command-specific diagnostics with
their owning V3-V8 batches.

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
5. `feat(diagnostics): add phase-aware diagnostics and recovery metadata`
6. `feat(tex-vm): introduce Eqtb and SaveStack for definitions and counts`
7. migrate dimen/skip/toks/catcode/mathcode/font assignment classes in bounded
   green commits
8. `feat(tex-vm): execute through a streaming mouth`
9. `feat(tokens): preserve file, revision, and expansion token origins`
10. `feat(tex-vm): support parameter text and command prefixes`
11. `refactor(tex-vm): centralize execution mode and nest state`
12. add stable event IDs from token/expansion origins, then migrate SemanticSink
   event families one vertical slice per green commit
13. `feat(checkpoint): add transactional continuation snapshots`
14. `refactor(ir): introduce SemanticDocumentIr metadata and frame builder`
15. `feat(layout): introduce renderer-neutral LayoutIr`

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
