# Private TFM parameter hardening TDD RED v1

This record preserves the prospective RED for the test-only parameter
whole-oracle/no-panic hardening authorized by Pro review
`6a93e9d1-5100-83ee-85a3-cb84f168bbf9`.

## Content identities

- Accepted pre-test `tfm_validation.rs` SHA-256:
  `83b35e9d74db22986f16c14032009fdb58345e65da419879f79eb01622560b5f`
- Formatted prospective-test `tfm_validation.rs` SHA-256:
  `20b3e1da27b06ab070ba2cdef2143fd8a262af3e73d2e9ef5e6240e30a4e5a1c`
- Already hardened AST policy SHA-256:
  `17205d74773bc7726e04436e1055501a019d9474be672dca09f3bbf6327a93f9`

The prospective source added four tests but no literal reference
implementation:

- exact declared and zero-filled slot identity for every `np=0..8`;
- 32,768 signed-slant high-byte/representative-byte/low-nibble cases;
- 512 deterministic sign-valid full-chain cases compared with an independent
  scaler under `catch_unwind`;
- 256 deterministic invalid-sign full-chain cases asserting no panic and the
  exact first invalid parameter.

## Exact RED

The focused command was:

```text
env CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 RUSTFLAGS='-C debuginfo=0' CARGO_TARGET_DIR=/data/latexd-w25-target-20260821 cargo test -p tex-tfm-metrics tfm_validation::tests::parameter_shape_matrix_names_every_declared_and_filled_slot -- --exact
```

Compilation failed with exit status 101 and exactly five unresolved-reference
diagnostics:

```text
cannot find value `literal_signed_slant` in this scope
cannot find value `literal_signed_slant` in this scope
cannot find function `literal_scaled_parameter` in this scope
cannot find function `literal_signed_slant` in this scope
cannot find function `literal_scaled_parameter` in this scope
```

The first draft also contained one test-harness numeric-conversion error. It was
fixed before this authoritative RED and recorded in root `MISTAKES.md`; the
rerun above failed only for the two deliberately absent literal references.

No non-building RED commit was created. Production parameter code, its private
signature and output shape, immutable artifacts, and zero-caller policy were
unchanged.

## GREEN outcome

Generated test-only hardening is GREEN: the `np=0..8` slot matrix, 32,768 slant
low-nibble cases, 512 sign-valid independent-scaler cases, and 256 invalid-sign
first-failure cases run the whole private chain under `catch_unwind`.
Arithmetic/indexing no-panic explicitly excludes allocator exhaustion, and
production code remains zero caller. Final test-bearing source SHA-256 is
`33da91f8a9dd058ec1839a8ef65f0b3e7acc915625866ed5d7b17d18b8e2a717`.
The production prefix before `#[cfg(test)]` is byte-identical to commit
`edc60c1` at SHA-256
`a35dc3f3c982dc925c67df4caad591cd013aca40a6bd4a4eec8eaf3ed6fa376c`.
This content-addressed RED/GREEN record is
`docs/evidence/tex-tfm-parameter-hardening-tdd-red-v1.md`.
