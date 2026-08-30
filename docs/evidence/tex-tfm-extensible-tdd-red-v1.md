# Private TFM extensible prospective RED evidence v1

The RED tests were added on exact commit
`842a2c61dd1b86183fd30ff002431a3c2b0d6545`, tree
`21d55d192b01d309cc617989de65002ed289b7e4`, before any extensible production
type, error, state, or transition existed.

The pre-test source digests were:

- `crates/tex-tfm-metrics/src/tfm_validation.rs`:
  `3883c90f95865262df95f4073f705189f52316e354a1b5fcde51f39948076a60`;
- `crates/tex-tfm-metrics/tests/subset_boundary.rs`:
  `325902c3d9e130ccf277ef252ac64275c6a871969c808d384dd72029b2d146f2`.

After adding and formatting only prospective tests and structural policy
expectations, their digests were respectively
`2bb6c60960cc660e0b73f7a604461569c467a16817c4c15c97b11abd43d32b2e` and
`603949a341a50e9b81b41a29074197c9b5bd33e29337d6146663edd866f80768`.
The child test module's explicit `use super::{...}` list already named every
prospective symbol before RED capture.

Cargo used `CARGO_INCREMENTAL=0`, dev/test debug info 0,
`RUSTFLAGS='-C debuginfo=0'`, and
`CARGO_TARGET_DIR=/data/latexd-w25-target-20260821`.

## Missing production behavior RED

Command:

```text
cargo test -p tex-tfm-metrics --lib extensible_
```

Result: exit 101. Rust reported unresolved imports for all five planned
production symbols:

```text
no `CheckedExtensibleRecipe` in `tfm_validation`
no `ExtensibleCheckedTfm` in `tfm_validation`
no `ExtensiblePart` in `tfm_validation`
no `ExtensibleValidationRule` in `tfm_validation`
no `check_extensibles` in `tfm_validation`
```

The prospective tests already cover the exact by-value signature, raw source
contract identity, empty and valid typed recipes, optional top/middle/bottom
existence, mandatory repeat including character zero, first recipe and field
order, unreferenced invalid recipes, exact `ne=32753`, parameter/suffix
isolation, predecessor and raw-allocation retention, all eight extensible-owned
native witnesses, and parameter-owned pass-through.

## Structural registry RED

Command:

```text
cargo test -p tex-tfm-metrics --test subset_boundary staged_validator_ast_has_only_private_items_and_no_production_references -- --exact
```

Result: exit 101. The exact mismatch was:

```text
left: ["check_preamble_header", "check_characters", "check_boxes", "check_lig_kern", "check_kerns"]
right: ["check_preamble_header", "check_characters", "check_boxes", "check_lig_kern", "check_kerns", "check_extensibles"]
```

The structural mutant test was also run before implementation and passed. The
existing generalized policy already rejected the new
`ExtensibleCheckedTfm` Clone derive, alternate returner, arbitrary attribute,
Debug derive, and production `include!` cases. The implementation must satisfy
the new exact registry with one constructor/returner and zero production
references without weakening those mutants.

No non-building RED commit was created. This evidence records chronological
commands and content identities; it is not an externally signed CI event.
