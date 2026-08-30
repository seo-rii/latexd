# Private TFM whole-chain oracle prospective RED v1

The prospective test-only whole-chain contract was added on the clean pushed
baseline commit `84fa6f398420853b1fe1c6fedceac2c5111e3ead` and tree
`41582dda2876e38fdee4eadd66aed6f6ed4128ee`. The test-bearing source before
implementation has SHA-256
`1be767540a0b88c8e9bab5b559ab153750d2fb3cd191e1cf23834792d9b1401c`.
The production prefix before `#[cfg(test)]` remains byte-identical to the
reviewed parameter baseline at SHA-256
`a35dc3f3c982dc925c67df4caad591cd013aca40a6bd4a4eec8eaf3ed6fa376c`.

The prospective tests require exactly the review-authorized test-local
surface:

- `WholeChainOutcome`, with typed variants for size, header, character, box,
  lig/kern, kern, extensible, and parameter failures plus acceptance;
- `validate_whole_chain_for_oracle(Arc<[u8]>, i32) -> WholeChainOutcome`;
- exact outcomes and effective v4 owners for all 83 content-addressed native
  witnesses;
- 512 deterministic multi-defect frames covering every adjacent staged-order
  boundary and parameter source order under `catch_unwind`;
- 512 deterministic arbitrary byte/effective-size inputs under
  `catch_unwind`, with exact size-precondition and short-preamble outcomes.

The authoritative RED command was:

```text
env CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 RUSTFLAGS='-C debuginfo=0' CARGO_TARGET_DIR=/data/latexd-w25-target-20260821 cargo test -p tex-tfm-metrics --lib whole_chain
```

Compilation failed with only `E0425` and `E0433`: the test-local
`WholeChainOutcome` type and `validate_whole_chain_for_oracle` function did not
exist. Rust reported 72 missing-type/function uses. There was no production
symbol, caller, visibility, ownership-contract, loader, or integration change.
The test-only driver and outcome are the minimum implementation that can make
this prospective contract compile and run.

## GREEN closure

The minimum implementation is confined to the existing `#[cfg(test)]` module.
It runs the existing validators in the exact reviewed order, returns only a
test-local typed outcome, and discards the accepted parameter proof state. The
final test-bearing source SHA-256 is
`45d1e8b576752981c46142935e40e32311747c804b8d84911ae27fd9d51bcb1d`;
the production prefix remains unchanged at the hash above.

GREEN evidence covers 83/83 exact native outcomes and their effective v4
owners, 512 deterministic multi-defect staged-order cases, and 512
deterministic arbitrary byte/size cases. The package suite is 134/134 unit,
6/6 integration, 8/8 boundary, and 3/3 compile-fail doctests. Package
all-target Clippy and canonical workspace lib/bin Clippy are GREEN.
The full Python policy/oracle discovery is 156/156, including fresh
`pdftex -ini` TFM validity and box-scaling runs. The focused ledger policy is
58/58, and the standalone checker reports 33 rules, 83 witnesses, and the
v2-to-v3-to-v4 transition chain. Rustfmt, Python compileall, and diff checks are
GREEN.

Production policy remains 7/0/7. No `CompleteCheckedTfm`, `finish_validation`,
production caller, visibility change, loader, materializer, or integration
surface was added. The production marker remains blocked pending a new narrow
review.
