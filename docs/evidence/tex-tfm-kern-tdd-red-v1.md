# Private TFM kern phase TDD RED evidence

Date: 2026-08-30

This note preserves the prospective fail-first evidence required by lig/kern
replacement Pro review `6a93bc49-6f74-83ee-b517-7f02fcebb9f9`.
No non-building RED commit was created. The repository TDD contract requires the
test to fail before implementation, so the exact pre-fix sources, commands, and
diagnostics are content-addressed here before adding any production kern state.

## Pre-fix source identities

- `crates/tex-tfm-metrics/src/tfm_validation.rs` SHA-256:
  `fa3cbfd93cd19b47182be11b1bfa382b8fe4da29f373c55461c3a25d348b5074`.
  This module contains both the new unit tests and the still-unmodified
  production validator above `#[cfg(test)]`.
- `crates/tex-tfm-metrics/tests/subset_boundary.rs` SHA-256:
  `b894741a032c1438cc18462d9e9b38e9a3739aa01649d85c05e193f2e252e947`.
  This integration test contains the expected successor registry and the
  prospective macro-expansion mutants.

Both hashes were computed after `cargo fmt --all` and before implementing
`KernCheckedTfm`, `KernValidationRule`, `check_kerns`, or the AST policy
mitigations.

## Unit/type boundary RED

Command, with the repository's shared low-debug Cargo environment:

```text
env CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 RUSTFLAGS='-C debuginfo=0' CARGO_TARGET_DIR=/data/latexd-w25-target-20260821 cargo test -p tex-tfm-metrics --lib kern_entrypoint_consumes_only_the_lig_kern_state
```

Exit status: `101`. The compiler reported eleven unresolved successor
references. Representative exact diagnostics were:

```text
cannot find type `KernCheckedTfm` in this scope
cannot find type `KernValidationRule` in this scope
cannot find value `check_kerns` in this scope
```

The unresolved references occur in the signature gate, behavioral tests,
persisted-corpus ownership test, and shared test constructor. No assertion was
removed or weakened after this result.

## Structural boundary RED

The expected-constructor registry command was:

```text
env CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 RUSTFLAGS='-C debuginfo=0' CARGO_TARGET_DIR=/data/latexd-w25-target-20260821 cargo test -p tex-tfm-metrics --test subset_boundary staged_validator_ast_has_only_private_items_and_no_production_references
```

Exit status: `101`. Its exact current-side value was:

```text
left: ["check_preamble_header", "check_characters", "check_boxes", "check_lig_kern"]
```

The expected side additionally contained `check_kerns`.

The prospective mutant command was:

```text
env CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 RUSTFLAGS='-C debuginfo=0' CARGO_TARGET_DIR=/data/latexd-w25-target-20260821 cargo test -p tex-tfm-metrics --test subset_boundary structural_policy_rejects_alias_wrapper_reexport_macro_and_visibility_mutants
```

Exit status: `101`. After existing mutants failed as intended, the first new
unclosed path produced the exact assertion:

```text
missed non-private syntax in #[forge] struct KernCheckedTfm;
```

The same test also requires rejection of a production `include!` invocation
and an unapproved derive on the new proof state. GREEN must come from adding the
single private successor and fail-closed policy, not from changing these mutant
expectations.
