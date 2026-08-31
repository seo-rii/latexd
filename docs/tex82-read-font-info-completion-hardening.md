# TeX82 private TFM completion-hardening design

Status: marker implemented; production caller design pending.

This document preserves the reviewed historical design and records its
subsequent closure. The private zero-rule marker was implemented in `a98dd1c`;
it does not authorize a production caller or visibility change. Reverting that
commit returns the validator to the reviewed 7/0/7 baseline at `4f238e8`.

## Authority and exact source boundary

The compatibility authority remains the 1,031,999-byte official TeX82
`tex.web` object with SHA-256
`c62ab513ef167e93f71a23bd34f311e243210afd7c7a0f9b779614b71e398324`.
The already immutable parameter contract owns lines 11188..11199. A fresh
retrieval of the same official object established the following excluded
successor ranges, with trailing newlines included in each digest:

| Meaning | Official lines | SHA-256 |
| --- | --- | --- |
| Completion context | lines 11201..11225 | `56b9b2bc3e1abc3a4038be04d625744b1aaa9cb479a8a2ba25e074d8d77931ba` |
| Adjustment macro and complete final-adjustment block | lines 11205..11225 | `a61aaf83a618d111bbb29851ca91276222c71ccb05935569da6929f2cd08a0cc` |
| Parameter count, defaults, boundary state, and identifiers | lines 11209..11221 | `7736555b6caa8e959fbd25cd4473c36ae1eac417ff70951485eaabf918a91869` |
| Base-address correction and global allocation commit | lines 11222..11225 | `746e39abeb536d1b1cd4ddbe4eb9c7a43716ba79efd0684e042177fc466be4b3` |

TeX says the necessary TFM checks are complete before this block. The block
then writes `font_params`, default hyphen/skew characters, boundary-character
addresses, font name/area/domain, memory-base corrections, and global font
memory pointers. These final adjustments are excluded from private validity
completion because they materialize TeX runtime font memory and caller/default
state; they neither validate new TFM bytes nor define a new source rule. A
future loader/materializer must own them under its own source, default-state,
allocation, and rollback contract.

## Current ownership is already total

The v2 -> v3 -> v4 transition chain assigns all 33 inventoried validation
rules. The effective final counts include `ParameterCheckedTfm: 3` and
`TailCheckedTfm: 0`. Header already owns declared-frame availability and suffix
semantics, including `TFM-EOF-001..002`. There are no unowned post-parameter
validity rules and no justification for moving an existing rule again.

Consequently, the completion transition has owned rule IDs: none. It did not
create a v5 rule-ownership move merely to rename or duplicate the already
accepted v4 projection. The additive completion evidence pins v4, every source
contract, the accepted parameter source hash, and the excluded final-adjustment
ranges above without editing v1/v2/v3/v4 bytes.

## Exact marker proof state

The implemented successor shape is
`CompleteCheckedTfm { predecessor: ParameterCheckedTfm }`.

The implemented constructor is
`finish_validation(ParameterCheckedTfm) -> CompleteCheckedTfm`.

This constructor is deliberately infallible and read-free:

- it accepts exactly one by-value `ParameterCheckedTfm`;
- it performs no raw-byte, count, range, effective-size, EOF, suffix, path,
  default-state, resolver, cache, allocation, or global-memory read;
- it recomputes no parameter or predecessor semantic value;
- it stores only the complete predecessor and introduces no duplicate digest,
  count, table, slant, dimension, or error;
- it has one authorized struct construction and no derive, conversion,
  serialization, manual/inherent impl, unsafe block, or alternate constructor.

The marker means only “the reviewed private validation chain completed.” It is
not a loaded font, native-validity claim outside the inventoried contract,
materialized TeX memory object, public metric API, or permission to consume the
state in production.

Before the marker work unit, no `CompleteCheckedTfm` or
`finish_validation` symbol existed. Its prospective compile RED and
AST-registry RED were captured after design approval and before implementation;
the exact evidence is linked from the closure section below.

## Whole-chain outcome and first-failure order

Before any production caller, a test-only whole-chain driver may consume raw
test input and map each current typed error into a test-local outcome. It must
invoke the existing stages without changing them in this exact order:

```text
SizePrecondition -> Header -> Character -> Box -> LigKern -> Kern -> Extensible -> Parameter
```

The driver must stop at the first returned failure. It must not call later
stages after an error, preserve or recover a partially consumed proof state,
flatten errors into strings, or call the implemented marker. The
current header-first declared-frame proof intentionally precedes later semantic
rules even where a streaming native loader might encounter another defect
first.

The frozen native corpus has 83 witnesses. Whole-chain parity separates two
claim classes:

- single-owner native witnesses must accept or reject at the effective v4
  owner with their exact typed payload and must agree with the persisted native
  observation;
- multi-defect generated cases validate the staged order above against an
  independent test model, but must not be advertised as native streaming
  diagnostic precedence.

This distinction prevents a multi-defect input from silently redefining either
the staged Rust order or TeX's observable recovery order.

## Native oracle and content addressing

The authoritative refresh remains `scripts/check_tfm_validity_oracle.py` using
fresh `pdftex -ini`. Its artifact must record the resolved executable path,
full version, executable SHA-256, locale/timezone, source and blob identities,
exit status, normalized diagnostics, and observations. The checked-in v2
manifest and content-addressed blobs remain the Rust test input; a Python gate
refreshes the native report and compares exact expected observations.

The Rust test-only whole-chain driver must not spawn pdfTeX or infer validity
from a box-scaling proxy. The native producer and private consumer remain
independent, while the manifest joins them by exact blob SHA-256 and rule
ownership.

## Generated no-panic model

`catch_unwind` means no Rust unwinding panic from arithmetic, indexing,
slicing, conversion, explicit `unreachable!`, source-order iteration, or
collection length for the exercised bounded inputs. It covers:

- arbitrary byte arrays across preamble and declared-frame boundaries with
  legal and illegal effective sizes;
- structurally consistent generated frames that reach each later phase;
- sign-valid and injected-invalid box, kern, extensible, and parameter data;
- minimum, transition, and absolute maximum geometries already identified by
  phase tests;
- exact error-stage and first-failure assertions for rejected cases.

Allocator exhaustion, process abort/kill, stack exhaustion outside the
nonrecursive validator, hardware faults, and compiler/runtime corruption are
outside this no-unwind claim. In particular, `catch_unwind` must not be used to
claim that OOM is recoverable. The no-panic claim excludes allocator exhaustion.

Generators must have frozen seeds, bounded case counts, deterministic byte
construction, exact effective-size sets, and independently computed expected
outcomes. A failure report must include the seed, case index, effective size,
declared counts, first expected owner, and blob digest so it is replayable.

## AST and caller transition

The accepted pre-marker policy was exactly
`7 definitions, 0 references, 7 constructions`. Test helpers live under
`#[cfg(test)]` and do not change production counts.

After the separate Pro review authorized only the zero-caller marker, the
implemented policy became exactly
`8 definitions, 0 references, 8 constructions`: the eighth
definition/returner/construction is `finish_validation` and
`CompleteCheckedTfm`. Out-of-line production child modules remain forbidden.

A later private whole-chain production entrypoint would create real references
to the staged functions and is a different ownership transition. It requires a
new caller-count contract, prospective RED, exact error sum type, rollback, and
another review. The zero-reference assertion must never be weakened to “any
number of private calls.”

## Ordered marker implementation gates

1. The design and document assertions were committed with no Rust production
   change.
2. A separate Pro review covered the zero-rule marker, final-adjustment
   exclusion, whole-chain oracle, no-panic model, and AST transition.
3. Test-only whole-chain parity and generated no-panic tests were added while
   production counts remained 7/0/7.
4. Package tests, boundary/AST tests, compile-fail doctests, native oracle,
   ledger, package and canonical workspace Clippy, rustfmt, compileall, and diff
   checks passed.
5. After explicit approval, prospective missing-symbol and 7-to-8-registry RED
   were recorded before the private zero-caller marker was added as commit
   `a98dd1c`.
6. Another narrow review remains required before any production function calls
   the marker or any earlier validator stage.

## Rollback and prohibited scope

The marker rollback baseline is `4f238e8`. Reverting `a98dd1c` removes the
private marker and its tests/documents without changing the accepted private
parameter implementation, ownership v1/v2/v3/v4, source contracts, corpus, or
public dimension-subset API.

No production caller is authorized. No public or crate visibility is
authorized. Loading, final adjustment/materialization, resolver/cache ownership,
serialization, persistence, VM integration, checkpoint changes, W3 activation,
snapshot epoch changes, and a public facade remain blocked. Any proposal that
crosses one of these boundaries requires a separate Pro review and a new
decision-complete packet.

## Design review closure

Completion design Pro review `6a93f688-9360-83e8-bcbd-25848721b9bf`
returned `PROCEED_PRIVATE_TFM_WHOLE_ORACLE` at confidence 0.93. It authorizes
only a test-only whole-chain driver with exact single-owner native witnesses
and independently modeled multi-defect generated cases. Current production AST
counts remain 7/0/7; the production marker remains blocked. See
`docs/evidence/tex-tfm-completion-design-pro-closure-v1.md`.

The completion-hardening whole-chain oracle is test-only and GREEN. It checks
83/83 exact native outcomes with typed payloads and effective v4 owners, plus
512 multi-defect staged-order cases and 512 arbitrary byte/size cases under
bounded `catch_unwind`. Final test-bearing source SHA-256 is
`45d1e8b576752981c46142935e40e32311747c804b8d84911ae27fd9d51bcb1d`;
the reviewed production prefix is unchanged and AST counts remain 7/0/7. The
production marker remains blocked. Prospective RED and GREEN identities are in
`docs/evidence/tex-tfm-whole-oracle-tdd-red-v1.md`.

Whole-oracle closure review `6a93fc44-71f8-83ee-ba6a-f4df2fa5bc1c`
returned `PROCEED_PRIVATE_TFM_ZERO_RULE_MARKER` at confidence 0.91. It
authorizes only the private zero-rule marker and an exact AST transition from
7/0/7 to 8/0/8 after prospective compile/registry RED. The marker must be
root-private, read-free, single-field, and zero-caller; the
production caller remains blocked. See
`docs/evidence/tex-tfm-whole-oracle-pro-closure-v1.md`.

The private zero-rule completion marker is implemented with exactly one
`ParameterCheckedTfm` predecessor field and one read-free shorthand
construction. Structural policy is GREEN at 8/0/8 and rejects body/signature,
field, visibility, derive/impl, alias/macro, unsafe, and alternate-constructor
mutants. Validator source SHA-256 is
`b9404150ae8b5e450fb4c0facb2fedff27cbc784cc602c4e0b50d5b5c4a6c56b`;
policy SHA-256 is
`6fce21c7c47e172d315f5b74bb20194ad0f131020d3958bed0ff675863ea91cc`.
The production caller remains blocked. See
`docs/evidence/tex-tfm-completion-marker-tdd-red-v1.md`.
