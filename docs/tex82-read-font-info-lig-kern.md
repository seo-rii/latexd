# TeX82 `read_font_info` lig/kern instruction contract

## Authority and focused source pins

The compatibility authority remains the official TeX82 `tex.web`, whose full
SHA-256 is
`c62ab513ef167e93f71a23bd34f311e243210afd7c7a0f9b779614b71e398324`.
The helper at lines 11150..11154 has SHA-256
`50b7893997fe98c90314983b83456c0fa15f577d02e91e2a03cf2a8034765c63`.
It defines existence as both an unsigned-byte range check and an existing
character record, rather than range membership alone. The instruction and
boundary block at lines 11156..11172 has SHA-256
`a105c3b6349d6ad4c15e37f3cc0d8b64670c14ffc3f79cdd827da05043d28c5d`.

These pins are machine-recorded by
`tfm-validation-rule-transition-v2.json`. That transition retains the reviewed
v1 contract byte-for-byte and changes only the owner of `TFM-KERN-001` from
`LigKernCheckedTfm` to the later `KernCheckedTfm`.

## Exact source order

The private instruction validator must preserve this order:

1. initialize the boundary-label sentinel and an absent boundary character;
2. when `nl=0`, skip the complete instruction loop and retain both sentinels;
3. otherwise decode every four-byte instruction in table order;
4. for `skip_byte > 128`, first range-check the 16-bit restart target, then set
   the boundary character only when `skip_byte=255` on the first instruction;
5. for an ordinary instruction, check the next character unless it equals the
   installed boundary character, then check either the ligature replacement's
   existence or the kern index, then check a nonterminal forward skip;
6. after the complete loop, derive the boundary label only when the final
   instruction has `skip_byte=255`.

The boundary character is therefore available to later ordinary instructions,
but a first-instruction marker is not itself an ordinary instruction. The
terminal boundary label uses the final instruction's already range-checked
restart target. No instruction branch reads or scales a kern fix word.

## Private successor boundary

The only authorized implementation consumes `BoxCheckedTfm` and returns one
private `LigKernCheckedTfm` containing the exact predecessor, typed decoded
instructions, optional boundary character, and optional boundary-program
start. It must not scale kern fix words. The implementation has no public or
crate-visible path and no production caller.

Kern scaling is a distinct reviewed successor. Only after lig/kern
closure may a private `KernCheckedTfm` consume `LigKernCheckedTfm`. Extensible
recipes, parameters, complete validation, source-visible font loading,
production ownership, checkpoints, and W3 remain blocked.

## Implemented evidence

The private `LigKernCheckedTfm` implementation now consumes and retains the
exact `BoxCheckedTfm`, stores every decoded instruction, and retains the
optional boundary character and boundary-program start. Its exact error space
preserves instruction index and source predicate identity. The corpus bridge
fixes 83/83 persisted corpus phase outcomes and
8/8 exact lig/kern-owned rejections while requiring `TFM-KERN-001` to succeed
through this phase.

An independent source-order oracle covers 4,096 generated programs in addition
to the complete selected single-instruction matrix. The
32,755-instruction absolute maximum proves the largest legal table geometry remains bounded. Mutated kern
words and raw suffixes preserve the instruction result, so
kern words remain unread and unscaled. The AST policy allows exactly one production construction
and authorized return path for the proof state and rejects alternate creation,
alias, macro, visibility, and clone paths. At that point a dedicated Pro closure
review was still required before any `KernCheckedTfm` implementation began.

## First closure review remediation

Closure review `6a93b53b-e6b0-83ee-92f5-686badb00774` returned
`REVISE_PRIVATE_TFM_LIGKERN` at confidence 0.94. It found no source-order or
state-machine defect, but rejected the evidence claim because the former
one-character fixture stopped at 32,753 instructions, the AST policy accepted
an unsafe `ptr::read` duplicate, and eight ledger IDs projected onto five error
variants without an explicit contract.

The replacement evidence uses the empty character domain and one word in each
required box table to reach the 32,755-instruction absolute maximum at
`lf=32767`. Deterministic restart, forward, and kern count-1/count cases cover
the upper arithmetic boundaries. The validator module forbids unsafe code and
the AST mutant gate rejects the exact `ManuallyDrop`/unsafe `ptr::read` path.
The v2 transition now contains `source_predicate_projections`: all eight active
lig/kern rule IDs occur exactly once, neither `TFM-KERN-001` nor an unknown ID
is admitted, and `TFM-LIGKERN-002`/`TFM-LIGKERN-008` intentionally share
`RestartTargetOutOfRange`. A replacement Pro review is required before kern
scaling.

## Replacement closure decision and kern source pin

Replacement review `6a93bc49-6f74-83ee-b517-7f02fcebb9f9` returned
`PROCEED_PRIVATE_TFM_KERN` at confidence 0.93. It found the maximum geometry,
unsafe construction, projection, canonical linkage, and high arithmetic
remediations sufficient. This permits exactly one root-private successor that
consumes the exact `LigKernCheckedTfm` by value; it does not permit any later
table, visibility, caller, or integration.

The immutable `tfm-kern-source-contract-v1.json` links the raw and canonical v2
transition and pins all source dependencies needed by that successor:

- fix-word meaning and `store_scaled`, lines 11108..11130, SHA-256
  `306907b8734bfa4dc990546e1fb84d0158c2b9af2338faed18808a06c4bfa58e`;
- effective-size normalization, lines 11142..11148, SHA-256
  `e4db0f873ddda4dc750831a8ddcb436bb44dae7cb41044314837a1895a9c1906`;
- the kern loop, lines 11173..11174, SHA-256
  `d1b13b62579f82c3fec9ea7fbf275c751ea1e7eb31a02c2d703233c7c84760f1`.

The loop scales the whole `nk` table in source order, including unreferenced
entries. It admits only sign bytes 0 and 255, applies the exact normalization
and nested divisions, and performs no entry-zero check. The private successor
must not read extensibles, parameters, or a raw suffix. Before implementation,
RED policy and behavior tests must cover macro-expansion construction paths,
all invalid signs, first-invalid order, exact scaling, and the distinct
`nk=32755` maximum. Another Pro closure review is mandatory after that one
transition.

## Implemented kern evidence

The strict-TDD private `KernCheckedTfm` implementation now consumes and retains
the exact `LigKernCheckedTfm`, applies the literal normalization and nested
division formula to the entire predecessor-bound kern range, and stores typed
scaled values. It neither changes the instruction/boundary state nor applies an
entry-zero rule.

Focused evidence covers 254 forbidden signs with exact index/sign payloads,
21 effective sizes × 10 fix words, first-invalid source order, the
32,755-word absolute kern maximum, all `TailCheckedTfm` witnesses passing
through, the same raw allocation, later-table and suffix isolation, and
no entry-zero check. The structural gate allows one constructor and zero callers
while rejecting production `include!` and unapproved proof-state attributes.

The prospective RED record is
`docs/evidence/tex-tfm-kern-tdd-red-v1.md`. Its pre-fix unit/source and AST policy
digests are respectively
`fa3cbfd93cd19b47182be11b1bfa382b8fe4da29f373c55461c3a25d348b5074` and
`b894741a032c1438cc18462d9e9b38e9a3739aa01649d85c05e193f2e252e947`, with the
exact unresolved-type and mutant diagnostics. The implementation remains
root-private and uncalled. Extensible, parameter, completion, loading, and all
integration work remain blocked until a dedicated kern closure review.

## Kern closure decision

Dedicated review `6a93c613-4678-83e9-abc3-1ce9d58da7d7` returned
`PROCEED_PRIVATE_TFM_EXTENSIBLE` at confidence 0.95. It found no remaining kern
arithmetic, whole-range, maximum-geometry, predecessor-provenance, isolation,
construction-path, or ownership defect. The complete content-addressed result
is indexed by `docs/evidence/tex-tfm-kern-pro-closure-v1.md`.

The authorization stops at one new root-private successor consuming the exact
`KernCheckedTfm` by value. Before implementation, a new immutable v3 ownership
transition must move only `TFM-EXT-001` and `TFM-EXT-002` from the current
effective `TailCheckedTfm` owner, the ledger must enforce the ordered cumulative
v2-to-v3 chain, and a focused source contract must pin full-`ne` iteration,
field order, optional-zero semantics, and mandatory-repeat existence. Parameter
rules, completion, visibility, loading, callers, and integration remain blocked
through a dedicated extensible closure review.

The first successor-preparation gate is now immutable. Transition v3 has raw
SHA-256 `5929817fa92f3f8ead2a05ba33476281bb16ab5661eef5926730fe6fa27ce09d` and
canonical SHA-256
`3206379d5f6f6748c2d532da83df565a187aee2077e936a67672336d10569ccf`.
It pins v2, adds only `ExtensibleCheckedTfm`, and moves exactly the two
extensible rules while preserving all parameter ownership. The ledger now
evaluates the ordered v2-to-v3 chain and fails on omission, reorder,
wrong-effective-owner, duplicate or repeated moves, and predecessor drift.
This ownership substrate does not authorize production extensible symbols; the
focused source contract and prospective RED evidence remain mandatory first.

The focused source prerequisite is now satisfied by immutable
`tfm-extensible-source-contract-v1.json`, raw SHA-256
`5ce088a9e04d5de598fbabd4d59347f0e7c089f7cb491ebffe83314d3fc9ebdd` and
canonical SHA-256
`e64c6d3d5afbf0349cab44eb22e57d0dc799786dbeddbc6c09c33e0f07dcb125`.
It pins existence lines 11150..11154 and the recipe loop lines 11176..11183,
distinguishes optional zero parts from mandatory repeat zero, requires complete
`ne` iteration, and excludes parameters and suffix. Its exact successful
maximum is `ne=32753`. The implementation and RED boundary now continue in
`docs/tex82-read-font-info-extensibles.md`; no predecessor code changes are
authorized.
