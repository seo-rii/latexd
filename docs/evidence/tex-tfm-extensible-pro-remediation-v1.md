# Private TFM extensible closure remediation v1

The dedicated ChatGPT Pro review ran against exact commit
`9c6be6e7c10e1b2601b5d8f375cd53020c91cc26` and tree
`58775e48ba292d109c53e19e186023edde83bb96`. `origin/main` matched that
commit and the tracked worktree was clean when the review packet was built.

- Chat UUID: `6a93d6b7-9f68-83e8-a8c1-86073852b953`
- Job ID: `review-20260830-160704-268c1d96`
- Verdict: `REVISE_PRIVATE_TFM_EXTENSIBLE`
- Confidence: `0.90`
- Request-file SHA-256:
  `4ddd9b6fd8b967879410bbf4194d9fbf393b05b02b368c8af73191ef59a141dc`
- Bridge content SHA-256:
  `398fcd4a0a7d0afeb650a10fffbda33e93eea783e003201e6deafb0bc4232fa4`
- Persisted rendered-review SHA-256:
  `d95d123cd8b2d6173efce7656c0feec9a632af30c62736f0934e7f4be4cc456b`

The review found no defect in `check_extensibles`, its typed output, exact
maximum geometry, source order, optional-zero versus mandatory-repeat
semantics, whole-table coverage, provenance, parameter/suffix isolation, or
the private no-caller AST boundary. It also accepted the cumulative v2-to-v3
ownership contents. No Rust source or immutable contract artifact needs a
change.

The sole blocking finding was that
`scripts/check_tfm_validation_ledger.py` could raise `TypeError` or
`AttributeError` for valid-JSON values with the wrong nested shape. The first
prospective test command covered v1 rule fields, v2/v3 projections, and chain
entries; it failed with 20 uncaught exceptions. A second top-level consumer
test failed with eight uncaught exceptions. The failures included
`rule_ids=null`, `rule_ids=1`, `rule_ids=[{}]`, object/array rule ids,
unhashable invariants, proof states, dependencies, witnesses, and owners, and
non-object transition or contract values.

The remediation adds container guards before `.get`, iteration, hashing,
`Counter`, `set`, dictionary construction, or membership operations. One
shared projection validator is used by both transition validators. Invalid
projection entries and rule-id lists now return stable policy errors; the
contract and transition-chain paths retain only validated string values for
set and dictionary operations. Source-contract and ledger consumers also
reject non-object contracts through their normal error lists. The existing
`contract=None` ledger API continues to mean “use the reviewed default
contract.”

The focused GREEN command now passes all four malformed-shape test methods.
The complete TFM Python gate passes 65 tests, including fresh pdfTeX box and
validity oracles. The standalone ledger still reports 33 rules, 83 witnesses,
the ordered v2-to-v3 chain, and active ownership counts LigKern 8, Kern 1,
Extensible 2, Tail 3. The unchanged Rust boundary passes 113 unit tests, 6
exact integration tests, 8 boundary/AST tests, and 3 compile-fail doctests.
Package Clippy with `-D warnings`, canonical workspace Clippy, Rust formatting,
Python compilation, and diff checks also pass.

This remediation does not authorize parameter work. A narrow renewed closure
review must confirm the checker-only diff and unchanged Rust/artifact boundary
before an immutable parameter ownership transition or `ParameterCheckedTfm`
work begins.
