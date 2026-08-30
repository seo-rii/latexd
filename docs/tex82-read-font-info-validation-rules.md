# TeX82 `read_font_info` validation rule ledger

## Status and authority

This ledger is the source-to-evidence map for a future TFM data validator. It is
not an implementation or a public proof type. Validator readiness review
`6a8ddef5-7b84-83e9-a8ff-b24a2c752739` returned `REVISE_TFM_PLAN` at
confidence 0.96 and authorized oracle closure plus this ledger only. The
implementation sequence remains private-first; no `font_load` module,
`ValidatedTfmAtSize`, resolver, owner, cache, VM integration, source primitive,
checkpoint capability, or epoch transition is authorized.

Phase 4 closure review `6a8e2bef-e164-83e8-99ee-be8002ced80f` returned
`PROCEED_PRIVATE_TFM_VALIDATOR` at confidence 0.89. It authorized only a
root-private preamble/layout/frame/header phase. Commit `962094c` implements
that phase as non-importable `HeaderCheckedTfm`; it is neither complete
validation evidence nor a public result. Character and later phases require a
new implementation gate.

Private-header implementation review
`6a8e358c-697c-83e8-a6ba-881a469553d7` returned
`REVISE_PRIVATE_TFM_HEADER` at confidence 0.94. It found no defect in the Rust
header checks, but found that the first machine ledger could pass after rule
semantics were reassigned between ids. Character work therefore remains
blocked on exact semantic-ledger and exact-byte corpus evidence.

Header-closure review `6a8e45fc-4bd0-83ee-b4f8-e2c948311ae1` returned
`PROCEED_PRIVATE_TFM_CHARACTER` at confidence 0.91 after that remediation. It
authorizes exactly one successor state: root-private `CharacterCheckedTfm`
must consume and retain `HeaderCheckedTfm` by value while validating character
records and bounded charlists. It does not authorize box scaling, table-entry
zero checks, lig/kern instruction interpretation, extensible recipes,
parameters, a public or crate-visible validator, production callers, font
ownership, resolution, caching, VM integration, or source-visible loading.
Exact header error attribution and independent v1 contract/character-owner
pins are required hardening before character closure, and a new Pro review is
mandatory before any later phase.

Character-closure review `6a939670-0fc8-83e8-923f-ebaed26b4c72` returned
`PROCEED_PRIVATE_TFM_BOX` at confidence 0.94. It found no blocking character
defect and authorizes only a private `BoxCheckedTfm` implementation that
consumes and retains `CharacterCheckedTfm`, uses its already-bound effective
size to scale every width/height/depth/italic word with exact TeX82 arithmetic,
and then checks the four scaled entry-zero values. Adjacent character-field
precedence, unreachable traversal-limit, structural privacy/reference, and
reviewed corpus/native-report bindings are mandatory evidence hardening before
box closure. Kern scaling and all lig/kern remains blocked, together with
extensible recipes, parameters, public or crate-visible validation, production
callers, font ownership/resolution/caching, source-visible loading,
checkpoints, and W3. A new Pro review is mandatory at the private box state.

Box-closure review `6a93a948-81a8-83ee-8173-a0a58dbe1a08` returned
`PROCEED_PRIVATE_TFM_LIGKERN` at confidence 0.95. It found no blocking box
defect and authorizes only source-ordered lig/kern instruction and boundary
state validation from `BoxCheckedTfm` to private `LigKernCheckedTfm`; kern
fix-word scaling is a separate successor after another closure review. The
native oracle must verify the base TFM SHA-256 before mutation, and the AST
policy must enforce exactly one production construction and return path for
each proof state. A later private `KernCheckedTfm` must consume the reviewed
lig/kern predecessor. The reviewed v1 machine contract remains immutable; its
combined `TFM-KERN-001` ownership must be split only by an explicit versioned
contract transition.

That immutable-v1 transition is now
`tfm-validation-rule-transition-v2.json`. It pins the exact focused lig/kern
instruction source SHA-256
`a105c3b6349d6ad4c15e37f3cc0d8b64670c14ffc3f79cdd827da05043d28c5d`
and moves only `TFM-KERN-001` to `KernCheckedTfm`. The complete source-order
contract is `docs/tex82-read-font-info-lig-kern.md`.

The private `LigKernCheckedTfm` implementation now consumes and retains the
exact box predecessor and validates the eight lig/kern-owned rules in source
order. Its executable evidence covers 83/83 persisted corpus phase outcomes,
8/8 exact lig/kern-owned rejections, 4,096 generated programs checked against
an independent oracle, and the 32,753-instruction maximum. Structural policy
allows exactly one production construction and authorized return path. The v2
split is enforced operationally: kern words remain unread and unscaled, and
`KernCheckedTfm` remains unimplemented pending the next closure review.

The required character evidence hardening is now executable. Exact private assertions
cover four adjacent metric precedence pairs, while exhaustive domains `1..=5` and 512
generated inputs assert that `CharListTraversalLimit` remains unreachable. A `syn` AST
policy and AST negative mutants reject function-pointer aliases, wrappers, re-exports,
macro references, and alternate item/field/associated/foreign visibility. Both Rust phase bridges pin the
reviewed v2 manifest
`db680c23a099b5b39c484d34c357116fc8d6967a9151db4108af0ddf4cfbb0be` and canonical
native fixture `9df44bf4b157acfb65fa0d5cc7de4d42ba7f869bae460e07daf984e1fbca19b4`;
the Python canonical-object pin rejects case-id, size, classification, first-rule, and
blob-mapping mutation. Ledger policy, native oracle, and Rust suite run in source order
inside one required CI job.

The compatibility authority is the official TeX82 source at
`https://tug.ctan.org/systems/knuth/dist/tex/tex.web`. The audited source has
full-file SHA-256
`c62ab513ef167e93f71a23bd34f311e243210afd7c7a0f9b779614b71e398324`.
The `read_font_info` region is lines 10870 through 11210 in that file and has
SHA-256
`57f665ae4cc87c721d444fdde0a1817f194f44bab18388c42a1d26d830c6ddc8`.
Native evidence comes from fresh `pdftex -ini -interaction=nonstopmode`
processes generated by `scripts/check_tfm_validity_oracle.py`.

The ledger covers TFM data classification for an already acquired immutable
byte object and an already resolved effective size. TeX file lookup, `at` or
`scaled` syntax recovery, global `font_info` memory exhaustion, `font_max`, and
logical font identity are outside that claim. The effective-size command cases
remain evidence for the future validator's input precondition, not malformed
TFM classifications.

## Public result policy

The future public result space is intentionally coarse:

- an effective size outside `1..2^27-1` maps to `InvalidEffectiveSize` before
  bytes are inspected;
- every TFM data rejection below maps to `MalformedTfm`;
- detailed rule identifiers remain private and non-normative;
- acquisition limits and resource budgets are caller-policy errors, never
  `MalformedTfm`;
- success can become public only after every private phase, native parity,
  deterministic properties, substitution tests, fuzz/no-panic gates, and a
  later implementation review pass.

## Source-order rule map

The “Rust phase” column names a future crate-private phase. No phase may return
an externally usable success object. Native witnesses are versioned case ids;
paired ids deliberately separate acceptance from rejection.

| Rule id | Source predicate and state | Dependency | Native evidence | Future private Rust phase and public result |
| --- | --- | --- | --- | --- |
| `TFM-SIZE-001` | The caller supplies a positive effective size below `2^27`sp. Invalid `at` syntax is corrected before TFM data loading. | Effective size; upstream scanner state. | `valid_cmr10`, `valid_cmr10_at_1sp`, `valid_cmr10_at_16sp`, `valid_cmr10_at_max_sp`; `invalid_at_size_zero`, `invalid_at_size_limit`. | Precondition before byte decoding; invalid input maps to `InvalidEffectiveSize`, not `MalformedTfm`. |
| `TFM-COUNT-001` | Each of the twelve size halfwords is read by `read_sixteen`; a first byte above 127 aborts. | Declared frame and EOF. | `size_field_high_bit`; all twelve positions remain a required generated Rust property. | Private preamble decoder; rejection maps to `MalformedTfm`. |
| `TFM-RANGE-001` | Reject `bc > ec + 1`. | Decoded `bc`, `ec`. | `invalid_character_range`; positive `empty_range_2_1`. | Private preamble/range phase; `MalformedTfm`. |
| `TFM-RANGE-002` | Reject `ec > 255` independently of the empty-range rule. | Decoded `ec`. | `character_range_ec256`. | Private preamble/range phase; `MalformedTfm`. |
| `TFM-RANGE-003` | If `bc > 255`, the only surviving source state is `bc=256, ec=255`, normalized internally to the canonical empty range. Other `bc=ec+1` forms are also valid. | Source-order range checks. | `empty_range_2_1`, `empty_range_256_255`. | Private normalization only; every `bc=ec+1` form remains a generated property before publication. |
| `TFM-GEOMETRY-001` | `lf` must equal six preamble words plus all decoded table lengths and the normalized character count. | Checked count arithmetic. | `aggregate_length_mismatch`; controls and both empty-range fixtures. | Private checked layout; `MalformedTfm`. |
| `TFM-GEOMETRY-002` | `nw`, `nh`, `nd`, and `ni` must each be nonzero even for an empty character range. | Size fields. | `zero_width_table_consistent`, `zero_height_table_consistent`, `zero_depth_table_consistent`, `zero_italic_table_consistent`. | Private checked layout; `MalformedTfm`. |
| `TFM-RESOURCE-001` | TeX may decline a valid font when global font count or `font_info` capacity is exhausted. | Global engine state, not input validity. | Deliberately outside the byte oracle. | Explicitly excluded from any future `ValidatedTfmAtSize` data-validity claim. |
| `TFM-HEADER-001` | The header must contain at least checksum and design-size words; later header words are ignored. | `lh`, declared frame. | `short_header`; `minimal_header_lh2`. | Private header phase; `MalformedTfm`. |
| `TFM-HEADER-002` | Design size decoding rejects a negative high halfword and values below 1pt; exactly 1pt is valid. | Header bytes. | `design_size_below_one_pt`, `design_size_exactly_one_pt`, `design_size_largest_positive`. | Private header phase; `MalformedTfm`. |
| `TFM-CHAR-001` | Every character record's width, height, depth, and italic indices must be below `nw`, `nh`, `nd`, and `ni`. The checks run even when width index is zero. | Character-table slice and metric counts. | `invalid_character_width_index`, `invalid_character_height_index`, `invalid_character_depth_index`, `invalid_character_italic_index`. | Private character phase using checked slices; `MalformedTfm`. |
| `TFM-CHAR-002` | Ligature and extensible tags must address indices below `nl` and `ne`. | Character tag and target byte. | `invalid_character_ligature_index`, `invalid_character_extensible_index`. | Private character phase; `MalformedTfm`. |
| `TFM-CHARLIST-001` | A list target must lie in `bc..=ec`; unlike ligature/extensible references, it is not required to denote an existing character. | Normalized character range. | `charlist_out_of_range`; accepted `charlist_target_in_range_absent`. | Private charlist phase must preserve range-only semantics; `MalformedTfm` only for range failure. |
| `TFM-CHARLIST-002` | Following smaller list targets must not return to the character currently being checked. | Stateful graph traversal and character order. | `valid_charlist_acyclic_chain`; `charlist_self_cycle`, `charlist_two_node_cycle`, `charlist_three_node_cycle`. | Private bounded graph phase; cycles map to `MalformedTfm`. |
| `TFM-BOX-001` | Every width/height/depth/italic fix word uses `store_scaled`; sign byte must be 0 or 255. | Effective size and exact nested arithmetic. | `invalid_width_fix_word_sign`, `invalid_height_fix_word_sign`, `invalid_depth_fix_word_sign`, `invalid_italic_fix_word_sign`. | Private exact-scaling phase; `MalformedTfm`. |
| `TFM-BOX-002` | Entry zero of each box table must be zero after scaling, not merely raw-word zero. | Effective size. | Natural-size `nonzero_width_zero`, `nonzero_height_zero`, `nonzero_depth_zero`, `nonzero_italic_zero`. | Private exact-scaling phase; `MalformedTfm`. |
| `TFM-BOX-003` | The identical nonzero raw entry-zero word can round to zero at 1sp and nonzero at 16sp. Validation evidence is therefore size-bound. | Exact effective size and `store_scaled`. | `nonzero_width_zero_at_1sp`/`nonzero_width_zero_at_16sp`, `nonzero_height_zero_at_1sp`/`nonzero_height_zero_at_16sp`, `nonzero_depth_zero_at_1sp`/`nonzero_depth_zero_at_16sp`, `nonzero_italic_zero_at_1sp`/`nonzero_italic_zero_at_16sp`. | Private at-size box phase; final proof must store the same size used here. |
| `TFM-LIGKERN-001` | `nl=0` skips the instruction loop and boundary state remains unset. | Instruction count. | Empty-range controls remove lig/kern data and load successfully. | Private lig/kern phase. |
| `TFM-LIGKERN-002` | For `a>128`, `256*c+d` must be below `nl`; an in-range restart is valid. | Instruction index and source order. | `valid_ligkern_restart`; `invalid_ligkern`. | Private state-machine phase; invalid restart maps to `MalformedTfm`. |
| `TFM-LIGKERN-003` | If the first instruction has `a=255`, byte `b` establishes the boundary character before ordinary instructions are checked. | First-instruction state. | `valid_boundary_character_absent_next_bypass`. | Private state-machine phase; state must be installed before next-character existence checks. |
| `TFM-LIGKERN-004` | An ordinary instruction requires next-character existence unless `b` equals the declared boundary character. | Boundary-character state and character existence. | Accepted `valid_boundary_character_absent_next_bypass`; rejected `ligkern_next_in_range_absent` and `invalid_ligkern_next_character`. | Private state-machine phase; ordinary failure maps to `MalformedTfm`. |
| `TFM-LIGKERN-005` | For `c<128`, the ligature replacement byte must denote an existing character. | Character existence, not range alone. | `invalid_ligature_target`, `ligature_target_in_range_absent`. | Private state-machine phase; `MalformedTfm`. |
| `TFM-LIGKERN-006` | For `c>=128`, `256*(c-128)+d` must be below `nk`. | Kern-table count. | `invalid_ligkern_kern_index`; existing valid cmr10 instructions. | Private state-machine phase; `MalformedTfm`. |
| `TFM-LIGKERN-007` | If `a<128`, the forward skip must remain inside the instruction table. | Current instruction index and `nl`. | `invalid_ligkern_skip`; ordinary cmr10 instructions. | Private state-machine phase; `MalformedTfm`. |
| `TFM-LIGKERN-008` | If the final instruction has `a=255`, its already range-checked restart target becomes the boundary label. | Terminal instruction state. | `valid_boundary_label`, `invalid_boundary_label`. | Private state-machine phase; preserve source ordering. |
| `TFM-KERN-001` | Every kern fix word uses the same sign-constrained exact scaler. | Effective size. | `invalid_kern_fix_word`; valid cmr10 kern table. | Private kern phase; `MalformedTfm`. |
| `TFM-EXT-001` | Nonzero top, middle, and bottom recipe bytes must each denote an existing character; zero means absent optional part. | Character existence. | Out-of-range `invalid_extensible_top`, `invalid_extensible_middle`, `invalid_extensible_bottom`; in-range-absent `extensible_top_in_range_absent`, `extensible_middle_in_range_absent`, `extensible_bottom_in_range_absent`; valid cmex10 zero optionals. | Private extensible phase; `MalformedTfm`. |
| `TFM-EXT-002` | Repeat is mandatory and must denote an existing character. | Character existence. | `invalid_extensible`, `extensible_repeat_in_range_absent`; valid cmex10 recipes. | Private extensible phase; `MalformedTfm`. |
| `TFM-PARAM-001` | `fontdimen1` slant is a signed pure number and uses a distinct decoding path. | Parameter index 1. | `signed_slant_parameter`. | Private parameter phase; preserve signed pure-number semantics. |
| `TFM-PARAM-002` | Every parameter after slant uses `store_scaled` and rejects other sign bytes. | Effective size and parameter index. | `invalid_fontdimen2`, `invalid_fontdimen5`, `parameter_8_invalid_fix_word`. | Private parameter phase; `MalformedTfm`. |
| `TFM-PARAM-003` | TeX validates all `np` supplied words, then zero-fills only missing standard parameters through seven. | `np`, EOF. | `short_np0`, `short_np4`, `short_np5`; `parameter_count_8_valid`, `parameter_8_invalid_fix_word`. | Private parameter phase; do not stop validation at parameter seven. |
| `TFM-EOF-001` | The declared frame must be fully readable through the final parameter word. | File buffering and declared extent. | `premature_eof`; exact controls. | Private frame/EOF phase; `MalformedTfm`. |
| `TFM-EOF-002` | Bytes after the declared frame are ignored semantically, including partial words, but remain raw provenance. | Raw length versus declared extent. | `trailing_word`, `trailing_1_byte_nonzero`, `trailing_2_bytes_nonzero`, `trailing_3_bytes_nonzero`, `trailing_long_nonzero`. | Private frame phase accepts suffix; future artifact retains raw bytes and separate raw/frame hashes. |

## Required private implementation sequence

The closure review authorized and `962094c` completed only the first structural
state below. Every intermediate representation remains root-module-private:

1. **Implemented privately:** effective-size binding, checked preamble, raw and
   normalized ranges, complete table layout, declared-frame availability,
   header/design-size validation, retained raw bytes, and typed raw/frame
   identities;
2. **Implemented privately:** character records, metric/tag index bounds,
   derived existence, range-only charlist targets, and bounded source-order
   charlist cycle validation;
3. **Implemented privately:** exact at-size box scaling and all four scaled
   entry-zero checks;
4. **Implemented privately:** source-ordered lig/kern instruction and
   boundary-state validation, returning private `LigKernCheckedTfm` without
   scaling kern words;
5. after a dedicated closure review, exact kern scaling that consumes
   `LigKernCheckedTfm` and returns private `KernCheckedTfm`;
6. extensible recipes, every supplied parameter, and EOF behavior;
7. whole-oracle parity and generated no-panic/property gates;
8. internal complete-state and A/B substitution closure;
9. only after every earlier gate passes, a separately reviewed public facade.

The future artifact must retain the exact immutable raw object used for frame
discovery, checks, hashes, and extraction. It must bind effective size, declared
frame extent and identity, full raw identity including ignored suffixes, design
size, and already-derived dimensions. It must have no public constructor,
deserializer, replacement bytes/size method, alternate-size scaler, or
conversion from `ExactTfmDimensionMetrics`.

## Remaining readiness evidence

The named native cases are a finite compatibility corpus, not an exhaustive
proof. The first private phase now deterministically covers all twelve high-bit
count positions, all 256 valid `bc=ec+1` empty ranges, every first-phase
truncation, generated suffix semantic invariance/raw-identity distinction, and
bounded arbitrary-input no panic. The character phase adds exhaustive
small-domain charlist graphs, full-domain bounds, and bounded arbitrary record
bytes; later-phase arbitrary inputs remain open. A versioned structure-aware
native differential corpus must cover size × table-zero, boundary-character × absent-next,
parameter-count × invalid-tail, and suffix-length interactions. Fuzzing is an
additional safety gate and cannot replace native parity.

The versioned content-addressed v2 corpus materializes the current 83 cases as
70 unique SHA-256 blobs. Its manifest records requested and resolved sizes, the
separate private-validator input size, normalized three-way classification,
first rejecting rule, and every supported rule. The native oracle and the Rust
header parity test both consume those persisted blobs; generation drift,
missing/corrupt/orphan blobs, case-key drift, or classification drift fails the
policy suite.

`tfm-validation-rules-v1.json` is the canonical machine contract for all 33
rules. It pins the audited source hashes, one source ordinal and unique anchor
per rule, hashes of the predicate/dependency/future-phase cells, symbolic
dependency ids, exact witness lists, and proof ownership. Source ordinal and
proof state are deliberately independent: the source-late EOF rules are already
proved by `HeaderCheckedTfm` because declared-frame availability is a structural
header invariant.

`scripts/check_tfm_validation_ledger.py` now enforces 33/33 exact semantic rule
cells, exact `HeaderCheckedTfm` proof ownership, unique rule ids and anchors,
contiguous source ordinals, closed symbolic dependencies and proof states,
known native witness ids, and a complete 83/83 fixture-case join. Its policy
suite fails on duplicate/swapped rows, predicate/dependency/witness
reassignment, proof-ownership reassignment, missing dependencies, unknown or
unmapped witnesses, missing documentation, and missing CI enrollment. The
native upper-positive design witness and Rust maximum-geometry/unavailable-frame
plus generated-consistent-preamble gates are now complete. The remediation
closure review authorizes only the private character/charlist successor stated
above. Character closure still requires exact variant attribution, immutable
contract/ownership guards, generated graph evidence, complete corpus phase
separation, and another review.
Current-font ownership, public validation, and source-visible font loading
remain separate blocked decisions.

The private `CharacterCheckedTfm` implementation consumes and retains the exact
header predecessor without replacement bytes, size, layout, or digest inputs.
It validates every record in character-code order, including all fields of
width-zero records, and never consults derived existence for charlist range or
cycle checks. The persisted corpus bridge now asserts 83/83 phase outcomes and
10/10 exact character-owned rejections while requiring every later-owned
malformed case to succeed through this phase. Generated evidence covers
exhaustive small-graph domains `1..=5`, full-domain chain/cycle bounds, 512
arbitrary-record no-panic inputs, compound private precedence, suffix/frame
identity, later-table isolation, and zero public or production reachability.
Character closure review authorized the next root-private state. The private
`BoxCheckedTfm` now consumes and retains the exact character predecessor,
scales every width/height/depth/italic word with literal source arithmetic, and
checks all four scaled entry-zero values only after the full scaling pass.
Exact tests cover every forbidden sign byte in every box table, source-order
precedence, 1sp/16sp entry-zero behavior, predecessor identity, suffix and
later-table isolation, maximum geometry, generated sign-valid input, and 83/83
persisted corpus phase outcomes. The AST policy rejects a `Clone` proof-state
mutant, and the ledger enforces exact `BoxCheckedTfm` proof ownership for
`TFM-BOX-001` through `TFM-BOX-003`.

The focused source contract and native exact-sp evidence are documented in
`docs/tex82-read-font-info-box-scaling.md`. A fresh pdfTeX INITEX matrix freezes
21 effective sizes × 10 fix words, including the size-normalization boundaries,
signed extremes, and nested-division carry cases. It separately records exact
signed width/italic and box-observed negative height/depth. The oracle runs in
the required CI job before the Rust suite and uploads engine, source, TFM, and
probe provenance. Box-closure review
`6a93a948-81a8-83ee-8173-a0a58dbe1a08` authorized only the split
`BoxCheckedTfm` to `LigKernCheckedTfm` instruction/boundary successor. Kern
scaling remains blocked until a dedicated lig/kern-closure review and must then
produce a separate `KernCheckedTfm`. Extensible recipes, parameters,
public/current-font ownership, source-visible loading, checkpoints, and W3
remain blocked.
