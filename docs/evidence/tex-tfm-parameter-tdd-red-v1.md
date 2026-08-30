# Private TFM Parameter Prospective RED Evidence

This record freezes the strict-TDD failure observed before any parameter
production symbol was added. The immediately preceding production source
`crates/tex-tfm-metrics/src/tfm_validation.rs` had SHA-256
`72deabc1701a0c156a105637272de48d7a0ec35aa87fbafd61ebc17cc3f2af45`.
The pre-edit AST policy
`crates/tex-tfm-metrics/tests/subset_boundary.rs` had SHA-256
`603949a341a50e9b81b41a29074197c9b5bd33e29337d6146663edd866f80768`.

The formatted prospective Rust test source, still with unchanged production
code above `#[cfg(test)]`, had SHA-256
`c10c863fb9d6baa0ab3264ec1bda7559d99831b75b53472dfd39652700516183`.
The prospective AST policy had SHA-256
`3bcaf9adb2949a6615f3543f907f794a7a841869f866b94155ddfd9d8676621e`.
Those tests bind immutable ownership v4 raw SHA-256
`edbccde695940a26634735f79bad60d64f8a11c63f8d48c927cfad194b4cd88e`
and parameter source-contract raw SHA-256
`223aad57857393d02096adbdaa9cc587be13c515e9e7e86e1b19454f0c8164dd`.

## Rust compile RED

The serial package command was:

```text
env CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 RUSTFLAGS='-C debuginfo=0' CARGO_TARGET_DIR=/data/latexd-w25-target-20260821 cargo test -p tex-tfm-metrics --lib parameter_entrypoint_consumes_only_the_extensible_state -- --exact
```

It failed with Rust error `E0432` and only the four prospective unresolved
imports:

```text
no `ParameterCheckedTfm` in `tfm_validation`
no `ParameterValidationRule` in `tfm_validation`
no `SignedSlant` in `tfm_validation`
no `check_parameters` in `tfm_validation`
```

The tests already required the by-value entrypoint, signed unscaled slant,
ordinary exact scaling, standard zero fill, all 254 forbidden non-slant signs,
source-order diagnostics through parameter eight, retained extra parameters,
the complete successful `np=32755` table and its last-word failure, suffix
isolation, same-`Arc` provenance, and all eight parameter witnesses.

## AST zero-caller RED

The independent integration-test command was:

```text
env CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 RUSTFLAGS='-C debuginfo=0' CARGO_TARGET_DIR=/data/latexd-w25-target-20260821 cargo test -p tex-tfm-metrics --test subset_boundary staged_validator_ast_has_only_private_items_and_no_production_references -- --exact
```

It compiled and failed at the exact entrypoint registry assertion:

```text
left: ["check_preamble_header", "check_characters", "check_boxes", "check_lig_kern", "check_kerns", "check_extensibles"]
right: ["check_preamble_header", "check_characters", "check_boxes", "check_lig_kern", "check_kerns", "check_extensibles", "check_parameters"]
```

The same prospective policy permits exactly one constructor for
`ParameterCheckedTfm`, rejects alternate construction/derivation/include paths,
requires inherited visibility, and requires zero production references to
`check_parameters`.

No non-building RED commit was created. Production implementation follows only
after these two independently reproduced failures; the final GREEN commit will
include tests, implementation, this evidence, and the phase-local documentation.
