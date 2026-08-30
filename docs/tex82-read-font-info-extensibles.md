# TeX82 `read_font_info` extensible-recipe contract

This document fixes the compatibility and proof boundary for the private
`KernCheckedTfm -> ExtensibleCheckedTfm` transition. It does not authorize
parameters, completion, visibility, callers, loading, caching, persistence, VM
use, checkpoints, or W3.

The immutable machine contract is
`crates/tex-tfm-metrics/tests/fixtures/tfm-extensible-source-contract-v1.json`.
Its raw SHA-256 is
`5ce088a9e04d5de598fbabd4d59347f0e7c089f7cb491ebffe83314d3fc9ebdd` and its
canonical SHA-256 is
`e64c6d3d5afbf0349cab44eb22e57d0dc799786dbeddbc6c09c33e0f07dcb125`.
It links both predecessor dimensions:

- ownership transition v3: raw
  `5929817fa92f3f8ead2a05ba33476281bb16ab5661eef5926730fe6fa27ce09d`,
  canonical
  `3206379d5f6f6748c2d532da83df565a187aee2077e936a67672336d10569ccf`;
- kern input source contract v1: raw
  `19d08087ce4b96bc4e3e9059e161adfd4705157e5a7e768190695155b7c9b2a1`,
  canonical
  `754519a85d9479c616fc2a246d6c584f839b617f43c68c4b3fa55c486e3a0b74`.

## Pinned source semantics

The authority is official TeX82 `tex.web`, full SHA-256
`c62ab513ef167e93f71a23bd34f311e243210afd7c7a0f9b779614b71e398324`.
The contract pins:

- `check_existence`, lines 11150..11154, SHA-256
  `50b7893997fe98c90314983b83456c0fa15f577d02e91e2a03cf2a8034765c63`;
- the complete extensible-recipe loop, lines 11176..11183, SHA-256
  `c155058da84f06e687bd1cf226e3fc9900280abb1e4e60783360cb31f8f0c7cc`.

Every declared recipe is read in table order. Its four bytes are interpreted
in source order as top, middle, bottom, and repeat. Zero top, middle, or bottom
means an absent optional part and bypasses existence checking. Repeat is
mandatory: its byte always passes through `check_existence`, so character code
zero succeeds only when character zero actually exists. Existence means the
character's decoded `char_info` has a nonzero width index, not merely that the
code lies inside `bc..=ec`.

The loop validates the whole `ne` table, including unreferenced recipes. It
must return the first missing field of the first invalid recipe and must not
read parameters or a raw suffix. It performs no scaling.

## Exact maximum geometry

A successful nonempty recipe table needs at least one existing character. The
smallest valid geometry uses two header words, one character-info word, two
width words so that the character can have a nonzero width index, and one word
for each required height, depth, and italic table. Together with the six count
words this is 14 non-recipe words. Therefore the absolute successful maximum is
`lf=32767, ne=32753`. A fixture with character zero existing and every recipe
equal to `[0, 0, 0, 0]` reaches that exact bound.

## Implementation gate

Prospective RED evidence must exist before production symbols. It must pin the
exact private signature, raw contract identity, optional-zero and mandatory
repeat behavior, recipe/field precedence, unreferenced invalid recipes, the
32,753-recipe maximum, parameter/suffix isolation, exact predecessor and raw
allocation retention, pass-through of parameter-owned corpus witnesses, and
one authorized construction path with zero production callers.

The implementation may add only a root-private checked-recipe representation,
`ExtensibleCheckedTfm`, an exact private error family, and one by-value
`check_extensibles` transition. It must not change `check_lig_kern` or
`check_kerns`, extract a shared existence helper, or begin parameter work. A
dedicated extensible closure Pro review is mandatory afterward.

## Implemented private evidence

The strict-TDD implementation adds one root-private `ExtensibleCheckedTfm`, a
typed checked-recipe record, exact optional-part/repeat errors, and one
`check_extensibles(KernCheckedTfm)` transition. It duplicates only the bitset
existence expression inside the new function and does not change or extract
logic from `check_lig_kern` or `check_kerns`.

Focused tests cover empty and valid recipes, every optional field, mandatory
repeat including character zero, first recipe and field order, unreferenced
invalid recipes, parameter and 8,193-byte suffix isolation, exact predecessor
and raw-allocation retention, 8/8 extensible-owned native witnesses, and every
parameter-owned pass-through witness. The successful maximum processes all
`ne=32753` recipes. A distinct empty-domain frame reaches the absolute declared
`ne=32755` geometry and rejects recipe zero's mandatory repeat without panic.

The exact prospective RED commands and identities are in
`docs/evidence/tex-tfm-extensible-tdd-red-v1.md`. Before implementation, the
source and AST test files had SHA-256 values
`3883c90f95865262df95f4073f705189f52316e354a1b5fcde51f39948076a60` and
`325902c3d9e130ccf277ef252ac64275c6a871969c808d384dd72029b2d146f2`.
Production remains zero-call and the proof state remains non-cloneable,
non-serializable, safe-Rust-only, and root-private. Parameter work remains
blocked until a dedicated extensible closure Pro review.

The completed gate is green with 113 crate unit tests, 6 exact integration
tests, 8 boundary/AST integration tests, 3 compile-fail doctests, and 65 TFM
Python tests including fresh pdfTeX box/validity oracles. The standalone ledger
reports 33 rules, 83 witnesses, ordered v2-to-v3 ownership, and both focused
source contracts. Package Clippy with `-D warnings`, canonical workspace
Clippy, formatting, Python compilation, and diff checks are also green.

## Initial closure review and checker remediation

Extensible closure review `6a93d6b7-9f68-83e8-a8c1-86073852b953`
returned `REVISE_PRIVATE_TFM_EXTENSIBLE` at confidence 0.90. It found no Rust
recipe-validation or proof-boundary defect. The only blocking issue was that
malformed but valid-JSON ledger shapes could raise from iteration, `Counter`,
`set`, dictionary construction, or membership operations instead of returning
controlled policy errors.

Strict RED tests reproduced 20 nested-shape exceptions and eight top-level
consumer exceptions. The checker now validates top-level objects, projection
lists and string rule ids, transition entries and ownership fields, and v1
rule/invariant/proof-state/dependency/witness fields before using them in those
operations. Existing v1/v2/v3 and source-contract bytes are unchanged, and the
valid chain retains the same owner counts. Exact review identities and the
remediation record are in
`docs/evidence/tex-tfm-extensible-pro-remediation-v1.md`. Parameter work remains
blocked pending a narrow renewed closure review.

Renewed closure review `6a93dda5-4d88-83e8-b9a9-55cc15d33187` returned
`PROCEED_PRIVATE_TFM_PARAMETER` at confidence 0.95. It closed the traceback
finding and required no Rust or existing immutable-artifact change. The next
authorization is limited to a new immutable parameter ownership transition,
an exact focused parameter source contract, prospective RED evidence, and one
root-private zero-caller by-value `ParameterCheckedTfm` phase. New schema
fields must use type-strict scalar checks and representative malformed-shape
tests should assert their category diagnostics. Exact review identities are in
`docs/evidence/tex-tfm-extensible-pro-closure-v1.md`. Completion and every
integration surface remain blocked through a dedicated parameter closure
review.
