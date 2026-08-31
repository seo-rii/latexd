# Private TFM zero-rule completion marker prospective RED v1

The prospective private completion-marker contract was added on the clean
pushed baseline commit `4f238e896f84da95e2adfbe421dcc923e8e69525` and tree
`24d6770c6e0c9fd723f2f6edc4f9cbe71d3bef32`. It implements the narrow
authorization from Pro review `6a93fc44-71f8-83ee-ba6a-f4df2fa5bc1c` only.

Before implementation, the test-bearing validator source SHA-256 was
`58c17c1bce565b727224db17f0349ab0c955ba80ad1fb39065fab8d7ec42b2e9`.
Its production prefix before `#[cfg(test)]` remained byte-identical to the
accepted parameter baseline at
`a35dc3f3c982dc925c67df4caad591cd013aca40a6bd4a4eec8eaf3ed6fa376c`.
The prospective AST policy SHA-256 was
`aab815a557f909ca10cb76ef6f09f754e6a8ebed65d42458068f50fbbe17ad37`.

## AST registry RED

The boundary policy first registered only the proposed eighth entrypoint and
proof state, the exact one-field predecessor shape, and rejection mutants for
derive, impl, extra/tuple/wrong/public fields, unauthorized returners, and
alternate construction. This command:

```text
env CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 RUSTFLAGS='-C debuginfo=0' CARGO_TARGET_DIR=/data/latexd-w25-target-20260821 cargo test -p tex-tfm-metrics --test subset_boundary staged_validator_ast_has_only_private_items_and_no_production_references
```

failed with the exact registry mismatch: actual entrypoint definitions were
the existing seven, while expected definitions appended only
`finish_validation`. Production references remained zero. No marker production
symbol existed.

## Missing-symbol compile RED

The `#[cfg(test)]` module then required the exact function type
`fn(ParameterCheckedTfm) -> CompleteCheckedTfm`, called it once, and asserted
that the retained raw `Arc` allocation remains pointer-identical through the
single predecessor field. This command:

```text
env CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 RUSTFLAGS='-C debuginfo=0' CARGO_TARGET_DIR=/data/latexd-w25-target-20260821 cargo test -p tex-tfm-metrics --lib completion_marker
```

failed only with `E0432`: unresolved imports `CompleteCheckedTfm` and
`finish_validation`. The minimum GREEN implementation is therefore exactly one
root-private single-field struct and one root-private read-free, infallible
constructor. The target policy is 8/0/8. No caller, visibility, ownership,
loader/materializer, persistence, VM, checkpoint, W3, epoch, or public API
change is authorized.

## GREEN closure and exact-body hardening

The minimum implementation adds only the private
`CompleteCheckedTfm { predecessor: ParameterCheckedTfm }` and the private
`finish_validation` function containing the single expression
`CompleteCheckedTfm { predecessor }`. The function has the exact by-value
signature, returns no error, and performs no read, branch, allocation,
conversion, or recomputation.

An additional prospective predecessor-field read mutant placed a field borrow
before the construction; the structural policy initially missed it and the
focused mutant test was RED. The GREEN policy now requires the exact plain
signature and exactly one predecessor-shorthand struct expression. It also
rejects attributes, reference or renamed arguments, extra statements, derives,
manual/inherent impls, alternate returners/constructions, extra/tuple/wrong/
public fields, aliases, macros, unsafe, and out-of-line child modules.

Final source identities are:

- validator source:
  `b9404150ae8b5e450fb4c0facb2fedff27cbc784cc602c4e0b50d5b5c4a6c56b`;
- production prefix through the marker:
  `3a49454c224a6453d023961a62faa792aa346bb14f533d7fc4712aab82742977`;
- structural boundary policy:
  `6fce21c7c47e172d315f5b74bb20194ad0f131020d3958bed0ff675863ea91cc`.

The package suite is GREEN at 135/135 unit, 6/6 integration, 8/8 boundary,
and 3/3 compile-fail doctests. The AST projection is exactly 8/0/8: eight
definitions/returners/constructions and zero production references. Immutable
v1/v2/v3/v4 ownership/source contracts are unchanged.

The full Python policy/oracle discovery is GREEN at 160/160, including fresh
`pdftex -ini` TFM validity and box-scaling runs. The standalone ledger checker
still reports 33 rules, 83 witnesses, and transition chain v2->v3->v4. Package
all-target and canonical workspace lib/bin Clippy, rustfmt, Python compileall,
and diff checks are GREEN.

The marker remains unreachable. A production whole-chain entrypoint, any
reference to `finish_validation`, loading/materialization, visibility, public
API, resolver/cache ownership, persistence, VM/checkpoint/W3, source activation,
and epoch changes remain blocked. The authorization requires
another review before any caller.
