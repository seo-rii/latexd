# M13.3-DP1 `\hangindent` Characterization

Status: characterization, passive W0 reader, source-unreachable W1 owner, and
W2 state persistence/semantic hashing complete; source activation not started.

This unit establishes the next bounded M13.3 assignment owner without changing
production behavior. `\hangindent` now has a dormant crate-private Eqtb owner,
but remains absent from builtin lookup, production command dispatch, and
rendering. Its state capability is supported, writable in a versioned snapshot
document, restorable, and included in semantic identity. Its command capability
remains inspect-only and unsupported. The production checkpoint writer remains
`LegacyOnly`, and the checkpoint VM semantic epoch remains 5.

## Why this owner

`\hangindent` has a fresh INITEX default of 0sp and has no current latexd
rendering or source-recovery consumer. Its dormant owner uses the neutral
`RawDimensionSp` scalar in a distinct `EqValue::DimensionParameter` variant, so
indexed dimension registers and the versioned wire-layer DTO remain separate.
`\hsize` and `\parindent` already occur in source heuristics, while box and font
state require identity and ownership designs larger than one scalar assignment
slice.

The plan review `6a807f92-8c54-83e8-8e47-21f683454768` approved only this
characterization gate. The passive wire contract and dormant in-memory owner
are now joined by a separately tested state persistence/hash unit. Source
activation with epoch 6 remains pending.

Gate-1 review `6a80898e-c504-83ee-ae9a-aa487dd8e8f3` returned `PROCEED` with
0.87 confidence after correcting two boundaries: durable state accepts every
signed `i32`, while command v1 freezes passive identity and owner linkage only.

W0 implementation review `6a80926f-ab9c-83ee-9115-c0186392de93` then returned
`REVISE` at 0.91 confidence. It found that V1 could follow future neutral-enum
growth and that command-only restore depended incidentally on
`UnknownPrimitive`. W0 now has an explicit V1 ID allowlist, a separate exact
command-v1 classifier, and typed unsupported-command restore preflight before
generic primitive resolution or mutation.

Closure review `6a809a1d-a964-83e8-bfd7-a20f038f4a6d` returned `APPROVE` with
0.94 confidence and closed all three original blockers. The state matrix also
rejects `parindent`, `hsize`, `HangIndent`, and `hang-indent` as raw V1 IDs.
The post-change full workspace suite passed before W1 began.

## Authoritative oracle

[`scripts/check_hangindent_oracle.py`](../scripts/check_hangindent_oracle.py)
runs every case twice in independent `pdftex -ini -interaction=nonstopmode`
processes: once with `\hangindent` and once with `\dimen0`. The 15-case matrix
covers defaults, optional equals and repeated signs, fractional-sp rounding,
all TeX82 physical units, cmr10 `em`/`ex`, true units at `\mag=2000`, internal
dimensions, `\the`/`\number`/`\ifdim`, grouping and `\globaldefs`, arithmetic,
odd signed division, `\afterassignment`, aliases, local shadowing, dynamic
lookup, malformed input, maximum dimensions, overflow, and divide by zero.

Error-sensitive cases run in fresh processes and preserve the final value,
diagnostic sequence, exit status, and a trailing sentinel. The CI
`hangindent-oracle` artifact also records the resolved engine path, full
version, executable SHA-256, exact source and source SHA-256 for every process,
raw combined output, invocation, locale, timezone, and normalization rules.
It also records the `kpsewhich` path, TEXMF search configuration, and the
lookup/resolved paths and SHA-256 of `cmr10.tfm`. The report asserts that all 30
expected owner/case processes ran. Under this exact oracle mode and listed
matrix, the two native owners match in every normalized case.

Notable native facts are:

- the default is 0sp and tested direct assignment is bounded at
  ±1,073,741,823sp;
- `.5sp` and `-.5sp` both become 0sp;
- cmr10 `1em` is 655361sp and `1ex` is 282168sp in this INITEX probe;
- `1truept` at `\mag=2000` is 32768sp;
- odd signed division truncates toward zero;
- repeated `\advance` can wrap the signed 32-bit scaled-point value;
- an oversized direct dimension stores the maximum and fires
  `\afterassignment` after the diagnostic;
- failed multiplication and division by zero preserve the previous value;
- `\dimexpr` is undefined in the TeX82 `pdftex -ini` target and is excluded
  from DP1 v1.

## Sharing decision

| Behavior | Native `\hangindent` | Native `\dimen0` | Current VM `\dimen0` | DP1 decision |
| --- | --- | --- | --- | --- |
| Default/scalar | 0sp, signed scaled points | Same | 0sp, `i32` scaled points | Use distinct `EqKey::DimensionParameter` and `EqValue::DimensionParameter(RawDimensionSp)` runtime storage; durable state accepts `-2,147,483,648..=2,147,483,647`. |
| Optional `=` and signs | `=` optional; repeated signs accepted | Same | Assignment currently requires `=` and its literal scanner consumes one sign | Parameter-local scanner policy required. |
| Fractional sp | `.5sp` truncates to 0sp | Same | `.5sp` rounds to 1sp | Do not reuse the register literal conversion unchanged. |
| Physical units | `pt`, `sp`, `in`, `pc`, `cm`, `mm`, `bp`, `dd`, `cc` | Same | Literal register scanner accepts only `pt` and `sp` | Activation is blocked until v1 either implements the native set locally or explicitly narrows the contract in a reviewed gate. |
| Relative/true units | `em`/`ex` read current font; `true*` reads `\mag` | Same | Register scanner has no font-relative or true-unit context | Requires an explicit current-font/`\mag` policy before activation. |
| Internal dimension | Accepted | Accepted | Accepted for dimension/skip registers and simple macros | Share only the proven internal-value branch through a typed lvalue. |
| Query/conditional | `\the`, `\number`, and `\ifdim` work | Same | `\the`/`\ifdim` exist, but `\number\dimen0` does not yield the native scaled value | Parameter-local query dispatch required. |
| `\dimexpr` | Undefined in this TeX82 target | Same | No builtin | Explicitly unsupported in DP1 v1. |
| Group/global state | Local unwind, global cancellation, both `\globaldefs` polarities | Same | Register Eqtb/SaveStack behavior exists | Reuse common ownership; the independent bounded dormant-owner model now passes. |
| Arithmetic | Advance/multiply/divide; signed odd division truncates; checked failure retains the old value | Same | Advance uses host addition, multiply saturates, divide-by-zero returns silently | Parameter-local arithmetic/error policy required; do not alter register behavior. |
| Recovery/hooks | TeX diagnostics, defined token progress, `\afterassignment` after scanner errors | Same | Several scanner failures return silently and do not share the native hook sequence | Parameter-local recovery tests are mandatory before source activation. |
| Layout effect | Native TeX consumes it in paragraph layout | Same scalar owner | No source-reachable `hangindent` owner or consumer exists | DP1 command v1 will deliberately remain storage/query-only; future layout consumption needs a new behavior capability and epoch. |

The current VM characterization also freezes its existing `\dimen0` behavior:
`1.5pt` becomes 98304sp, `.5sp` rounds to 1sp, and `-5sp/2` truncates to -2sp.
These are observations, not permission to change the existing register path.

## Passive W0 contract

W0 freezes both capability contracts without adding runtime application:

- `eqtb.dimension-parameter-state.v1`, whose v1 ID allowlist is exactly
  `hangindent`, whose scalar is exact signed `i32` scaled points, and whose
  direct-scanner range is deliberately not part of state v1;
- `primitive.dimension-parameter-command.v1`, whose contract is passive
  identity and owner linkage only. It specifies no scanner, unit, arithmetic,
  TeX query/expansion, grouping, diagnostic, hook, alias, or rendering behavior.

State layers reuse the exact canonical grammar of
`snapshot.rs:VmLayoutIntegerParameterStateV1` and its `validate` method:
nonempty state, full scope-depth lattice, strictly increasing owner IDs per
layer, root-default elision, and preservation of local default-valued shadows.
The typed ID and raw scaled-point value live in neutral
`dimension_parameter.rs`. The strict decoder accepts this state and validates
it before exposing it for inspection. A resolved `SnapshotMeaning::Primitive`
identity for this owner derives both capabilities, but raw source tokens and
character-source text do not. Neither capability is in the supported/writable
set: legacy and document serialization fail before emitting bytes, state
restore fails before interner mutation, and a passive command identity returns
typed `UnsupportedDimensionParameterCommand` before generic primitive lookup.
State plus command content deterministically reports the state error first.

State V1 validates against the exact public contract list
`DimensionParameterId::SNAPSHOT_V1_ALLOWED_IDS`; command V1 uses a separate
literal classifier rather than a general neutral-ID lookup. A later neutral ID
therefore cannot silently widen either V1 boundary. Repository inventory shows
that `LegacyVmSnapshotV1.scopes` is the only serialized carrier of resolved
`SnapshotMeaning`, while macros, tokens, queues, hooks and character sources
remain unresolved data and do not acquire this capability.

The frozen fixture and 12 Rust contract tests cover the complete signed
`i32` domain, canonical full-scope layers, local zero shadows, duplicate and
unknown fields/IDs, exact capability equality, identity-only command shape,
explicit direct/combined restore atomicity, zero-byte write failure, legacy-byte
projection, and source unreachability. “Canonical” here means the semantic JSON
data model, not byte-level JSON spelling. Capability headers retain the existing
set-membership rule: duplicate or noncanonical order is accepted and normalized
on a later supported rewrite rather than rejected by this W0 reader.

At W0, checkpoint epoch remained 5 and no dimension-state hash frame existed.
Production snapshots remain capability-free in the existing legacy byte/hash domain;
manually constructed passive snapshots are lane-suppressed and fail restore
preflight. W0 adds no `Primitive` variant, Eqtb key, source registration,
capture attachment, writer, runtime application, dimension-state hash framing,
or supported executable/writable capability.

## Dormant W1 owner

At W1, the unit added only `EqKey::DimensionParameter(DimensionParameterId)` and
`EqValue::DimensionParameter(RawDimensionSp)` plus crate-private Eqtb read and
assignment methods. The owner remains unreachable from source, is not a
`Primitive`, and was not captured by `VmSnapshot`. Root/global zero remains a
virtual default, local zero is retained as an owned shadow, local assignments
use the common SaveStack save-once/unwind behavior, and global assignments
cancel every pending restore for the key.

Implementation review `6a80ab98-2970-83ee-b698-1ca9ad642367` returned
`REVISE` at 0.95 confidence while approving the production storage design. Its
applicable evidence requests are closed in the staged implementation:

- an independent bounded reference model enumerates every valid trace of at
  most five operations and group depth at most two over begin/end, local/global
  assignment, and values `-1`, `0`, and `1`, comparing the effective value,
  exact materialized owner/level, and fully drained unwind;
- a production VM regression compares the complete `VmSnapshot` value and
  serialized `VmSnapshotDocument` bytes before and after private owner mutation
  and proves that neither dimension capability is derived;
- static carrier and symbol inventories find no serialized/hashable Eqtb enum,
  production source caller, snapshot attachment, writer, restore application,
  or hash frame for the dormant owner.

The post-remediation `tex-vm` and `tex-checkpoint` suites, canonical workspace
Clippy, rustfmt, and diff checks pass. A clean full-workspace run also passes,
including 239 latexd library tests, the 758/758 compiler integration target in
4509.05 seconds, 678/678 VM library tests, and every checkpoint/VM integration
target; the explicitly ignored large arXiv test remains a manual/nightly test.
Production snapshot and semantic-hash bytes are therefore unchanged, the
checkpoint epoch remains 5, and the non-test dead-code allowance is specific to
this deliberately source-unreachable W1 boundary.

Closure review `6a882770-9888-83ee-bba3-77d54be82343` returned `APPROVE` at
0.96 confidence. It requires no further W1 production change and approves this
unit for an independent commit/push. Its low residual risk is future accidental
coupling through a generic Eqtb enum consumer, so W2 and W3 must repeat the exact
`EqKey`/`EqValue` carrier audit and keep explicit snapshot projection.

## W2 persistence and semantic hash

W2 projects the private Eqtb owner and its SaveStack restore chain into
`VmDimensionParameterStateV1`. The projection keeps the complete scope lattice,
elides a root zero, preserves local zero shadows, orders owners canonically, and
returns no attachment when every layer is empty. `Vm::snapshot` now captures
that projection; versioned document serialization emits
`dimension_parameter_state`; restore validates all layers before mutation and
then rebuilds them root-to-leaf through the same typed assignment path.

Only `eqtb.dimension-parameter-state.v1` joins the supported/writable set.
`primitive.dimension-parameter-command.v1` remains readable-only, so a latent
command identity still fails document write and restore preflight before any
state or interner mutation. Legacy `VmSnapshot` serialization still rejects
capability-bearing state before writing bytes.

Checkpoint semantic identity preserves complete fingerprint v1 for every
previously supported state and selects an explicit complete fingerprint v2
domain only when dimension-parameter state is present. V2 appends the family
tag `eqtb.dimension-parameter-state.v1\0`, a fixed-width little-endian `u64`
JSON length, and canonical typed-state bytes. Golden hashes distinguish a root
owner, a local zero owner, and a changed local value; a frozen pre-W2
incomplete-v1 digest proves that a dimension-bearing snapshot is rekeyed. Raw
JSON whitespace and object-field order decode to the same semantic DTO, while
the DTO serialization and framing are themselves hash ABI. The production
checkpoint writer remains `LegacyOnly`: all capture categories suppress this
capability-bearing attachment, mark it non-replay-safe, and nevertheless retain
distinct state hashes and checkpoint IDs. Capability-free legacy hashes,
existing complete-v1 goldens, and `CHECKPOINT_VM_SEMANTIC_EPOCH` remain
unchanged.

TDD evidence includes the missing-projection compile RED and the pre-frame hash
collision RED. Focused projection, versioned round-trip/unwind, frozen W0/W2
contract, hash framing, and checkpoint-lane tests pass. Full `tex-vm` passes
681 library tests and every integration target; full `tex-checkpoint` passes 68
library tests and every compatibility/golden target. Canonical workspace Clippy
also passes. Exact carrier and symbol inventories show only the intended
projection/capture, document write/restore, and hash paths; no source, builtin,
`Primitive`, scanner, arithmetic, recovery, query, or renderer caller exists.

The clean full-workspace run immediately before the bounded Pro remediation
passed 239 latexd library tests, the 758/758 compiler integration target in
5931.38 seconds, 679/679 VM library tests, and every remaining target. After
remediation, the workspace excluding the `latexd` package passes completely,
the 239-test latexd library target passes independently, and the VM/checkpoint
counts above pass again. Two parallel full-workspace smoke attempts each hit a
different unrelated existing case once; both exact tests passed immediately in
isolation against the same binary and target directory. This cross-test flake
is tracked as `TEST-005` in the uncommitted risk register rather than reported
as a W2 success. The existing large arXiv corpus test remains explicitly
ignored for manual/nightly execution.

W2 Pro review `6a88490b-7640-83e8-aa45-8d9c50fc000c` returned `REVISE` at
0.94 confidence while finding the persistence architecture coherent and the W3
boundary intact. Its compatibility and failure-contract findings are closed as
follows:

- the former public `VmRestoreError::UnsupportedDimensionParameterState`
  symbol and display text remain as a deprecated source-compatibility shim, but
  supported dimension-state restore never emits it;
- restore precedence is an explicit four-cell matrix: valid state without a
  command restores, invalid state without a command reports invalid state,
  valid state plus a command reports typed unsupported-command, and invalid
  state plus a command reports invalid state because complete state validation
  precedes command preflight;
- a malformed duplicate in the deepest dimension layer fails before a fresh VM
  is built or a caller-owned interner is changed. After whole-snapshot
  validation, typed Eqtb assignments are infallible;
- repeated same-level locals followed by a nested global nondefault assignment
  cancel all pending restores. Projection, versioned restore, unwind, and
  recapture preserve the root owner and exact empty local layers;
- generic replay eligibility requires the attachment metadata flag, exactly one
  restorable attachment, continuation safety, matching complete VM hash,
  matching checkpoint ID, and a successful restore. A stale dimension
  attachment remains rejected until both its v2 VM hash and checkpoint ID are
  rekeyed, and any later state mutation invalidates it again.

Two attempts to submit the remediation closure packet on 2026-08-21 ended
before review when the shared browser broker crashed during Playwright startup
(`Browser.close: Connection closed while reading from the driver`). No closure
verdict was produced; this is recorded as review-infrastructure unavailability,
not as an approval or rejection. The prior findings and their local evidence
remain preserved in the plan and review packet.

## Next gate

Physical/current-font/true-unit scanning and exact parameter-local arithmetic/
recovery remain source-activation blockers, not passive identity blockers.
Characterization failure remains a valid reason to stop W3 and must not be
hidden by changing existing dimension-register semantics. Epoch stays 5 through
the source-unreachable owner and persistence promotion; epoch 6 remains atomic
with source reachability. W2 is complete. The pre-readiness W3 proposal bundled
the parameter-local scanner, arithmetic/recovery behavior, query semantics,
source activation, and epoch-6 transition as one atomic unit, without reusing
the current register dimension scanner unchanged.

Readiness review `6a8862e6-12a4-83e8-94ab-2cd2088661bd` subsequently returned
`REVISE_PLAN` (confidence 0.96): W3 is not authorized because the VM has no
durable current-font/metric context for `em`/`ex` and no operational `\mag`
context for true units. The intervening W2.5-SC0 characterization and contract
gate is documented in
[`m13-3-dp1-scan-context.md`](m13-3-dp1-scan-context.md). It changes no
production semantics, capability, snapshot, hash, writer, or epoch. Completion
of that evidence gate does not itself authorize W3; a later review must choose
the owner, persistence, capability, and epoch plan.
