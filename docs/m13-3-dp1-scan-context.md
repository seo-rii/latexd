# M13.3-DP1 Dimension Scan-Context Gate

## Status and boundary

Pro readiness review `6a8862e6-12a4-83e8-94ab-2cd2088661bd` returned
`REVISE_PLAN` with confidence 0.96. W3 remains blocked. The approved next unit,
W2.5-SC0, characterizes the current-font and magnification context required by
dimension scanning without changing production semantics. W2.5-SC0 completed
in `ae0858e`.

Post-SC0 architecture review `6a886eee-a3ac-83ee-bae5-603ff0aa2ea0`
returned `PROCEED_EXACT_TFM` with confidence 0.93. It selected one further
owner-neutral prerequisite: exact TFM design-size, x-height, quad, scaling, and
content identity from caller-supplied bytes. The additive `tex-tfm-metrics`
crate landed in `927b0dd`, with TeX82 short-parameter zero filling corrected in
`987cde0`; this does not authorize W3 or select a current-font owner.

Two W2.5 implementation-review submissions on 2026-08-22 ended before chat
creation when the shared browser broker exited during startup (`Browser.close:
Connection closed while reading from the driver`, then signal 15/broken pipe).
The live browser/authentication preflight passed between the attempts. No
implementation-review UUID or verdict was produced, so the failures are
recorded as review-infrastructure unavailability, not approval or rejection.
The exact-TFM implementation-review submission on the same date also ended
before chat creation when the shared broker exited during startup. A following
session probe reported `prior_exit_type=crashed`; it likewise produced no UUID
or verdict.

The replacement implementation review
`6a8db2a7-dd74-83ee-851b-4749f3f3fbd4` completed on 2026-08-26 with
`REVISE_BOUNDARY` at confidence 0.96. It found no independent offset, range,
scaling, signed-fix-word, indexing, hash, or denial-of-service defect in the
selected-field implementation. It required the broad public parse names to be
removed because subset extraction must not look like complete font-load
validation. The reviewed correction is limited to an explicitly named exact
frame dimension-subset API and does not authorize an owner or W3.

The following owner-readiness review
`6a8dbcc6-b010-83ee-8957-5cc4b352f136` returned `PROCEED_MAG_OWNER` at
confidence 0.94. It authorizes only a source-unreachable magnification owner:
grouped requested state, a non-grouped prepared latch, and optional durable
state under epoch 5. It does not authorize a `\mag` primitive, true-unit
scanner activation, font owner, source epoch change, or non-legacy writer.

MAG1 adds only the reviewed dormant Eqtb owner, snapshot family, data
capability, and semantic hash frame. It adds no source command, primitive,
scanner/renderer dependency, writer lane, layout consumer, or behavior
capability.
`\hangindent`, `\mag`, and font definition/selection remain unavailable as VM
production primitives. The checkpoint semantic epoch remains epoch 5, the
production checkpoint writer remains `LegacyOnly`, dimension-parameter state
remains supported, and the dimension-parameter command capability remains
readable-only and unsupported.

The compatibility target remains the full TeX82 behavior observed through
`pdfTeX -ini`. A pt/sp-only executable command is not covered by the existing
command identity or activation decision. Such a product would require a new
behavior capability, oracle, and epoch plan.

## Native evidence

[`scripts/check_dimension_scan_context_oracle.py`](../scripts/check_dimension_scan_context_oracle.py)
runs each of 26 cases in a fresh `pdftex -ini -interaction=nonstopmode`
process. The versioned expected result is
[`dimension-scan-context-oracle-v1.json`](../crates/tex-vm/tests/fixtures/dimension-scan-context-oracle-v1.json).
The report records the engine path, full version and SHA-256, invocation,
locale, timezone, `kpsewhich` path and TEXMF configuration, resolved paths and
SHA-256 values for `cmr10.tfm` and `cmr7.tfm`, every exact generated source and
source SHA-256, raw output, normalized diagnostics and observations, exit
status, and expected/observed process counts.

The matrix establishes these facts for the pinned target:

| Context | Native observation |
| --- | --- |
| Fresh INITEX font | `nullfont`; quad, x-height, `1em`, and `1ex` are all 0sp. |
| cmr10 | quad/`1em` = 655361sp; x-height/`1ex` = 282168sp. |
| cmr7 | quad/`1em` = 522469sp; x-height/`1ex` = 197518sp. |
| Grouped selection | cmr10 → grouped cmr7 → cmr10 restores both identity and metrics. |
| Scale 1200 / at 12pt | `1em` = 786434sp and `1ex` = 338602sp for both forms. |
| Font alias/dynamic lookup | `\let` preserves cmr10 identity; `\csname secondary\endcsname` selects cmr7. |
| Missing metric | Three diagnostics are emitted, the prior cmr10 current font remains effective, and the sentinel is reached. |
| Invalid at-size | `-1pt` diagnoses and recovers to cmr10 at 10pt; the sentinel is reached. |
| Magnification interaction | `\mag=2000` does not change cmr10 quad, x-height, `em`, or `ex`. |
| Fresh magnification | `\mag` is 1000. |
| Direct scan | 500, 1000, and 2000 store directly; optional equals and repeated signs are accepted. |
| Scope | Local/global assignments and both `\globaldefs` polarities follow ordinary Eqtb grouping. |
| Hooks/identity | `\afterassignment`, `\let`, and `\csname` are observable on success and recovery paths. |
| True units | At 500, `1truept` = 131072sp and `1truein` = 9472573sp; at 2000 they are 32768sp and 2368143sp. Ordinary units are unchanged. |
| Invalid magnification | 0 and 40000 can be stored before use; the first true-unit scan diagnoses and changes them to 1000. |
| Reassignment after use | Assigning 1000 after a true-unit scan under 2000 leaves the visible integer at 1000, but the next true-unit scan diagnoses and retains 2000 for conversion. |
| Legal preparation boundary | 32768 is accepted on the first true-unit scan and converts `1truept` to 2000sp. |
| Illegal preparation boundary | 32769 remains visible until first use, then diagnoses and is globally corrected to 1000. |
| Prepared latch and correction scope | Preparing 2000, requesting 1000, and using a true unit again retains 2000. The latch survives the group that prepared it, and its corrective assignment remains visible outside a later group even under `\globaldefs=-1`. |

The cmr7 cases are the anti-hard-coding witness: a scanner that substitutes
cmr10 values cannot pass the fixture. The 500/2000 true-unit cases similarly
reject an implicit magnification of 1000.

## Logical consumer contract

The future parameter-local scanner may consume a logical interface with this
shape; W2.5-SC0 deliberately does not choose Rust storage types or an owner:

```text
DimensionScanContext
    read_magnification() -> exact integer value or typed unavailable state
    read_current_font_id() -> stable identity or typed unavailable state
    read_current_font_metrics() ->
        quad_sp
        x_height_sp
        metric_provenance_identity
```

The scanner is only a consumer. It must not open font files, choose or mutate a
font, mutate magnification, consult unrecorded host defaults, or silently
substitute cmr10 or magnification 1000. An unavailable identity or metric is a
typed unavailable state with no fallback. Its future diagnostic, token
progress, assignment completion, stored-value, and `\afterassignment` behavior
must be fixed before source activation.

The required metric fields for DP1 are the current font's exact scaled-point
quad (`em`) and x-height (`ex`). Metric resolution and caching belong outside
the scanner. Any production metric provenance identity must at least bind the
logical font definition, effective scale, and metric content; a host path alone
is not stable semantic identity.

## Exact TFM prerequisite

`tex-tfm-metrics` exposes
`dimension_subset::extract_exact_frame`, which accepts exactly the byte frame
declared by a TFM `lf` field and returns `ExactTfmDimensionMetrics` with public
design-size, exact-frame SHA-256 identity, and `at_size_sp` projection. The
broad `parse_tfm` and `TfmParseError` names no longer exist. Rustdoc and
compile-fail tests make the promise explicit: Success does not imply that
TeX82 or pdfTeX would load the font.

The extractor parses only the aggregate layout needed to reach `fontdimen5`
and `fontdimen6`; it has no filesystem, resolver, Type1, renderer, VM,
font-selection, or floating-point dependency. Exact-frame length mismatch is
reported as `ExactFrameLengthMismatch`, which describes this subset policy and
does not classify the bytes as natively invalid. Inconsistent aggregate table
structure, invalid design size or selected fix-word range, and invalid
effective size remain typed errors with no fallback.

TeX82 accepts parameter tables shorter than seven entries and supplies zero for
the absent dimensions. Native INITEX and the crate therefore agree that `np=5`
retains cmr10 x-height but yields a zero quad, while `np=4` yields zero for both
x-height and quad. Absence is a valid zero-filled TFM state, not a missing-file
fallback or malformed-input error.

Scaling uses TeX82's nested integer `store_scaled` arithmetic rather than a
single rational multiplication. The distinction is observable at large odd
font sizes: native pdfTeX and the crate both report cmr10 quad `8388632sp` at
effective size `8388609sp`, while direct multiplication would produce
`8388633sp`. A signed fix-word boundary test also freezes TeX's negative-floor
behavior. The original oracle values remain exact:

| TFM and effective size | quad | x-height |
| --- | ---: | ---: |
| cmr10 natural | 655361sp | 282168sp |
| cmr10 at 12pt | 786434sp | 338602sp |
| cmr7 natural | 522469sp | 197518sp |

The exact-frame content identity is lowercase `sha256:` plus the digest of the
supplied frame bytes only and matches the audited classic-font manifest. Six
original exact-metric tests, four boundary tests, and two broad-symbol
compile-fail doctests pass. The boundary tests explicitly preserve `np=0`
zero-fill, reject a native-accepted trailing word as an exact-frame mismatch,
and accept an unrelated native-invalid `fontdimen2` mutation while returning
the unchanged selected dimensions. This is intentional evidence that the
result is not a font-load capability. In addition, 11 existing
`tex-fonts` tests, 681 `tex-vm` library tests and every VM integration target,
68 `tex-checkpoint` library tests and every checkpoint target, 239 `latexd`
library tests, the full 81-test Python policy suite, package Clippy, canonical
workspace Clippy, rustfmt, and diff checks pass. Existing IntegerParameter V1
and epoch/capability contracts remain unchanged.

This crate closes metric representation and conversion, not production
provenance. A later owner must still bind a logical font definition and
effective size to the TFM hash and restore that state deterministically.

## Full TFM validity gate

[`scripts/check_tfm_validity_oracle.py`](../scripts/check_tfm_validity_oracle.py)
runs 82 byte-frozen mutations in separate pdfTeX INITEX processes. Its expected
results live in
[`tfm-validity-oracle-v1.json`](../crates/tex-tfm-metrics/tests/fixtures/tfm-validity-oracle-v1.json).
Every case records its natural/explicit-at size, exact mutated TFM SHA-256,
source SHA-256, diagnostic, observation, exit status, and sentinel; the report
additionally preserves the engine path/version/SHA-256, base cmr10/cmex10
identities, raw output, locale, timezone, and process counts. The compatibility
source is the official
`tex.web` `read_font_info` region at
`https://tug.ctan.org/systems/knuth/dist/tex/tex.web`, pinned as full-source
SHA-256 `c62ab513ef167e93f71a23bd34f311e243210afd7c7a0f9b779614b71e398324`
and loader-section SHA-256
`57f665ae4cc87c721d444fdde0a1817f194f44bab18388c42a1d26d830c6ddc8`.
Two final 82-process reports are byte-identical with SHA-256
`bb48c1a684727289ff254c394faa5285595b3f5aed7663e28d0b717c45d7a4aa`.

The native matrix accepts unmodified cmr10, `np=5`, `np=4`, `np=0`, and one
complete trailing word after the declared length. It rejects a zero width-table
count, out-of-range character width index, charlist self-cycle, forbidden width
or kern fix-word sign, scaled-nonzero width[0], invalid unselected
`fontdimen2`, invalid selected `fontdimen5`, out-of-range lig/kern instruction,
and out-of-range extensible recipe. Every rejecting case reaches the sentinel
with `nullfont` and the exact `Bad metric (TFM) file` diagnostic.

Phase 1 source-inventory expansion covers every pre-table `read_font_info`
guard separately: a size-field high bit, invalid character range, aggregate
length mismatch, zero width/height/depth/italic table counts with compensating
geometry, a header shorter than two words, design size immediately below 1pt,
and premature EOF. All are rejected natively. The slant parameter's distinct
signed pure-number path is an acceptance witness: a sign byte rejected for
scaled font dimensions remains valid in `fontdimen1`.

Phase 2 covers the complete character-info index/tag tuple and the remaining
box-dimension symmetry. Height, depth, and italic indices at their table counts,
ligature and extensible tags at their table counts, and an out-of-range charlist
target all reject. Independent height/depth/italic forbidden-sign mutations and
nonzero entry-zero mutations also reject, matching the existing width witnesses.

Phase 3 closes the remaining lig/kern instruction and extensible-recipe
branches. Independent mutations reject an out-of-range next character,
ligature target, kern-table index, and forward skip as well as invalid top,
middle, bottom, and required-repeat recipe characters. Together with the
existing restart-index, kern-fix-word, and repeat-character witnesses, this
closes the targeted natural-size rejection-branch corpus. It does not establish
complete at-size validator compatibility.

Validator readiness review `6a8ddef5-7b84-83e9-a8ff-b24a2c752739` returned
`REVISE_TFM_PLAN` at confidence 0.96. It confirmed that validation must bind the
effective font size: the same nonzero table-zero fix word scales to zero and is
accepted at 1sp, but scales nonzero and is rejected at 16sp. It authorized only
oracle closure and a pinned source-rule ledger, not Rust validator code or a
public proof type.

Phase 4 completes the requested finite oracle closure with a per-case
natural/explicit-at schema. Valid cmr10 controls cover 1sp, 16sp, and the
`2^27-1`sp maximum; zero and `2^27`sp freeze TeX's invalid-at-size recovery.
Paired width/height/depth/italic entry-zero mutations use identical TFM bytes:
all four load at 1sp and reject at 16sp with the exact bad-TFM recovery.
`bc=2,ec=1` and `bc=256,ec=255` empty ranges load, while `ec=256` rejects.
One-, two-, three-, and 8193-byte nonzero suffixes all load, as do the minimum
two-word header, exact-1pt design size, and a valid eighth parameter; an invalid
eighth parameter rejects.

The stateful witnesses accept an acyclic charlist, a lig/kern restart, boundary
character bypass, and a valid boundary label. Two- and three-node charlist
cycles and an invalid boundary label reject. An in-range absent charlist target
loads because that rule is range-only, while ordinary lig/kern next and
ligature targets and every extensible-recipe reference to an in-range absent
character reject because those rules require character existence. The pinned
source predicates, dependencies, witnesses, future private phases, and coarse
error mapping are recorded in the
[`tex82-read-font-info-validation-rules.md`](tex82-read-font-info-validation-rules.md)
source-rule ledger.

This is a targeted finite compatibility corpus, not an exhaustive validity
proof. Deterministic generated properties, structure-aware differential cases,
fuzz/no-panic evidence, private whole-oracle parity, and substitution closure
remain later implementation/publication gates. Another Pro readiness review of
the Phase 4 fixture and ledger was therefore required before any private Rust
validator work; no public proof type was authorized by the earlier review.

Phase 4 closure review `6a8e2bef-e164-83e8-99ee-be8002ced80f` returned
`PROCEED_PRIVATE_TFM_VALIDATOR` at confidence 0.89. It found no
decision-changing source predicate missing from the first structural phase and
authorized only an unreachable root-private preamble/layout/frame/header unit.
Commit `962094c` adds private `HeaderCheckedTfm` state that consumes and retains
the exact `Arc<[u8]>`, binds the effective size, preserves both raw counts and
the normalized character domain, owns one checked complete table layout and
declared endpoint, separates raw-object and declared-frame digest types, and
retains raw/projected design size. It neither calls nor changes the public
dimension-subset extractor.

The strict-TDD gate first failed on the absent private module and absent
behavior API. The implementation then passed 34 private unit tests covering
all twelve count high-bit positions, all 256 native empty ranges, every
first-phase truncation, generated suffix invariance and raw/frame identity
separation, exact header/design boundaries, pointer-identical byte ownership,
bound-size retention, later-phase-invalid input acceptance, and bounded
arbitrary-input no-panic behavior. The package also passes the existing six
exact-metric tests, five subset-boundary tests, three doctests, package Clippy,
canonical workspace Clippy, the focused eight-test Python policy, and the
unchanged 82-process native report hash above.

This state proves only a checked structural header boundary; it is not full TFM
validity and is not importable outside its private root module.
[`scripts/check_tfm_validation_ledger.py`](../scripts/check_tfm_validation_ledger.py)
originally parsed the 33-rule table and checked order and global witness
coverage. Private-header implementation review
`6a8e358c-697c-83e8-a6ba-881a469553d7` returned
`REVISE_PRIVATE_TFM_HEADER` at confidence 0.94: the Rust header checks appeared
correct, but the original checker could remain green after moving predicate,
dependency, and witness cells to the wrong rule ids.

The corrected fail-closed policy uses
[`tfm-validation-rules-v1.json`](../crates/tex-tfm-metrics/tests/fixtures/tfm-validation-rules-v1.json)
as the canonical 33-rule semantic contract. It pins the TeX source hashes,
unique source anchors, 33/33 semantic rule cells, symbolic dependencies, exact
witness lists, and proof ownership. A source ordinal records where a rule occurs
in `read_font_info`; proof ownership independently records which private type
has already established it. In particular, source-late `TFM-EOF-001` and
`TFM-EOF-002` belong to `HeaderCheckedTfm`, not a later phase. Policy tests now
reject semantic-cell swaps, dependency-only and witness-only swaps,
header proof-ownership reassignment, duplicate or reordered ids, unknown or
unmapped witnesses, missing dependencies, documentation drift, and missing CI
enrollment. The v2 exact witness join is 83/83; the original v1 native report
remains the frozen 82-process characterization.

The content-addressed v2 corpus stores all 83 cases.
It uses 70 unique SHA-256 blobs. Each manifest entry binds the requested size,
native `resolved_effective_size_sp`, explicit `validator_input_size_sp`, normalized
`AcceptedByNativeLoader`/`MalformedTfm`/`InvalidEffectiveSize` result, first
rejecting rule, and every rule the case supports. A null resolved size means
natural-size loading rejected before design size became effective; the
separate validator input uses the documented 10pt harness size for those byte
checks. Invalid explicit sizes retain the original invalid validator input but
record native TeX's resolved 10pt recovery.

The Python policy verifies all case keysets, hashes, mutation equivalence, and
zero orphan blobs. Fresh native processes load the persisted blobs instead of
rerunning mutations, while the private Rust header corpus test loads them too.
Both consumers use the same persisted bytes. Rust verifies each digest and all
83 classifications against the machine contract's exact `HeaderCheckedTfm`
proof ownership. The supplemental native case accepts design fix word
`0x7fffffff`, resolves to `2^27-1`sp, and freezes quad `134218095sp` and x-height
`57788153sp`. Rust also accepts the exact `lf=32767` maximum frame, rejects that
frame one byte short, and accepts 128 generated structurally consistent
preambles. These boundary remediations are complete; a new Pro closure review
`6a8e45fc-4bd0-83ee-b4f8-e2c948311ae1` returned
`PROCEED_PRIVATE_TFM_CHARACTER` at confidence 0.91. It authorizes only a
root-private `CharacterCheckedTfm` that consumes and retains the approved
header state while checking character metric/tag indices and bounded charlist
range/cycles. Exact header error attribution, the reviewed v1 contract digest,
and the exact four character proof owners are required hardening before that
phase closes. Box scaling and all later TFM tables remain blocked, as do every
public or production caller, owner, resolver, cache, source loader, and W3. A
new Pro review is mandatory after character closure. Structured differential
evidence, fuzzing, and substitution closure remain later private gates; public
and downstream use remains forbidden.

The private `CharacterCheckedTfm` implementation now consumes and owns the
exact `HeaderCheckedTfm`, decodes typed records in character-code order, checks
all metric and tag indices even for width-zero records, derives existence only
after validation, and applies range-only, bounded source-order charlist cycle
checking. The persisted bridge proves all 83/83 phase outcomes and
10/10 exact character-owned rejections. Generated evidence covers exhaustive
small-graph domains `1..=5`, the full 256-code chain and cycle, 512 arbitrary-record
no-panic cases, compound error precedence, suffix/frame identity, and isolation
from every later table. Static gates retain zero public visibility and zero
production callers. Until the mandatory character-closure review,
box scaling remains blocked.

Character-closure Pro review `6a939670-0fc8-83e8-923f-ebaed26b4c72`
returned `PROCEED_PRIVATE_TFM_BOX` at confidence 0.94. It authorizes only one
private `BoxCheckedTfm` implementation that consumes and retains the exact
character predecessor, scales every width/height/depth/italic word at the
already-bound effective size with source-faithful TeX82 arithmetic, and then
checks the four scaled entry-zero values in source order. Before box closure,
the character evidence must add adjacent field-precedence pairs, an exact
unreachable-traversal-limit property, AST-based visibility/reference policy,
and direct reviewed manifest/native-report bindings. Kern scaling and all
lig/kern remains blocked, as do extensible recipes, parameters, any public or
crate-visible API, production callers, font ownership/resolution/caching,
source-visible loading, checkpoints, and W3. Another Pro review is mandatory
at the private box state before any later table phase.

The prescribed character hardening now covers four adjacent metric precedence pairs
with exact private variants. Exhaustive domains `1..=5` and 512 generated inputs assert
that `CharListTraversalLimit` remains unreachable. AST negative mutants in a `syn`
syntax-tree policy reject function-pointer aliases, wrappers, re-exports, macro
references, and alternate item/field/associated/foreign visibility. Both Rust phase bridges directly pin
the reviewed v2 manifest
`db680c23a099b5b39c484d34c357116fc8d6967a9151db4108af0ddf4cfbb0be` and canonical
native fixture `9df44bf4b157acfb65fa0d5cc7de4d42ba7f869bae460e07daf984e1fbca19b4`;
the Python canonical-object pin separately rejects case-id, size, classification,
first-rule, and blob-mapping mutations. Ledger policy, the native oracle, and the Rust
suite execute in source order inside one required CI job.

The authorized private `BoxCheckedTfm` is now implemented. It consumes and
retains the exact character predecessor, scales the complete
width/height/depth/italic interval with literal TeX82 `store_scaled` arithmetic,
then checks all four scaled entry-zero values in source order. Exact Rust
evidence covers all forbidden sign bytes, compound precedence, size-bound zero
rounding, predecessor identity, maximum geometry, suffix/later-table isolation,
generated sign-valid inputs, and all 83/83 persisted corpus phase outcomes.
The AST gate rejects a `Clone` proof-state mutant, while the ledger pins
exact `BoxCheckedTfm` proof ownership to its three source rules.

The focused source contract is
`docs/tex82-read-font-info-box-scaling.md`. Its native pdfTeX INITEX oracle
freezes 21 effective sizes × 10 fix words, including every size-reduction
boundary and signed/carry extremes. It distinguishes exact signed width/italic
from box-observed negative height/depth, and CI uploads the complete engine,
source, TFM, and probe provenance before running Rust tests. This remains a
private implementation pending a new Pro box-closure review. Kern scaling and
all lig/kern remains blocked, together with extensible recipes, parameters,
public or crate-visible APIs, production font ownership/resolution/caching,
source-visible loading, checkpoints, and W3.

Box-closure Pro review `6a93a948-81a8-83ee-8173-a0a58dbe1a08` returned
`PROCEED_PRIVATE_TFM_LIGKERN` at confidence 0.95. It found no blocking box
defect and authorizes exactly one successor: instruction and boundary-state
validation from `BoxCheckedTfm` to private `LigKernCheckedTfm`, without kern
fix-word scaling. The native oracle now verifies the base TFM SHA-256 before
probe mutation, and the AST policy requires exactly one production construction
and one authorized return path for each proof state. After a dedicated
lig/kern-closure review, exact kern scaling may proceed separately from
`LigKernCheckedTfm` to private `KernCheckedTfm`. Extensible recipes,
parameters, public or production integration, source loading, checkpoints,
and W3 remain blocked.

The immutable-v1 transition is machine-recorded in
`tfm-validation-rule-transition-v2.json`. It pins the focused instruction
source SHA-256
`a105c3b6349d6ad4c15e37f3cc0d8b64670c14ffc3f79cdd827da05043d28c5d`
and moves only `TFM-KERN-001` to the future kern state. The exact source order
and private boundary are documented in
`docs/tex82-read-font-info-lig-kern.md`.

The authorized private `LigKernCheckedTfm` implementation now consumes and
retains `BoxCheckedTfm`, decodes every instruction in source order, and checks
restart, first boundary character, ordinary next/ligature/kern-index/skip, and
the final boundary label. Evidence fixes 83/83 persisted corpus phase outcomes,
8/8 exact lig/kern-owned rejections, 4,096 generated programs against an
independent oracle, and the 32,755-instruction absolute maximum. The AST gate permits
exactly one production construction and authorized return path. In this phase,
kern words remain unread and unscaled; kern scaling and every later table or
integration remain blocked pending the dedicated closure review.

Lig/kern closure Pro review `6a93b53b-e6b0-83ee-92f5-686badb00774` returned
`REVISE_PRIVATE_TFM_LIGKERN` at confidence 0.94. It found the algorithm
source-faithful but blocked closure on the understated maximum, an
unsafe `ptr::read` duplication path, and absent many-to-one rule projection. The
remediation adds the empty-domain 32,755-instruction absolute maximum, exact
restart/forward/kern count-1/count boundaries, module-level unsafe prohibition
with the executable mutant, and total unique v2 `source_predicate_projections`.
No `KernCheckedTfm` work begins before a replacement closure review.

Replacement closure review `6a93bc49-6f74-83ee-b517-7f02fcebb9f9` returned
`PROCEED_PRIVATE_TFM_KERN` at confidence 0.93. Its authorization is limited to
one root-private successor consuming the exact lig/kern state. The immutable
`tfm-kern-source-contract-v1.json` pins fix-word scaling lines 11108..11130 and
SHA-256 `306907b8734bfa4dc990546e1fb84d0158c2b9af2338faed18808a06c4bfa58e`,
normalization lines 11142..11148 and
`e4db0f873ddda4dc750831a8ddcb436bb44dae7cb41044314837a1895a9c1906`, and the
kern loop lines 11173..11174 and
`d1b13b62579f82c3fec9ea7fbf275c751ea1e7eb31a02c2d703233c7c84760f1`.
The successor must scale the whole `nk` table in source order and preserve the
no entry-zero check boundary. Prospective RED evidence must first cover
`include!`/attribute-macro construction, all signs, and the independent
`nk=32755` maximum; every later table and integration remains blocked through a
dedicated kern closure review.

The strict-TDD private `KernCheckedTfm` implementation now consumes and retains
the exact lig/kern predecessor and scales only its complete kern range. Focused
evidence covers 254 forbidden signs, 21 effective sizes × 10 fix words,
first-invalid source order, the 32,755-word absolute kern maximum, pass-through
for all `TailCheckedTfm` witnesses, the same raw allocation, later-table and
suffix isolation, and no entry-zero check. Structural policy rejects
production `include!` and unapproved proof-state attributes. The exact RED commands and
pre-fix digests are recorded in `docs/evidence/tex-tfm-kern-tdd-red-v1.md` as
`fa3cbfd93cd19b47182be11b1bfa382b8fe4da29f373c55461c3a25d348b5074` and
`b894741a032c1438cc18462d9e9b38e9a3739aa01649d85c05e193f2e252e947`.
The state remains private and uncalled; no later phase starts before a
dedicated kern closure review.

Kern closure review `6a93c613-4678-83e9-abc3-1ce9d58da7d7` returned
`PROCEED_PRIVATE_TFM_EXTENSIBLE` at confidence 0.95. It found no blocking kern
arithmetic, whole-range, maximum-geometry, provenance, isolation, or
construction-policy defect. The next authorization is limited to one private
successor consuming the exact kern state. Before implementation, an immutable
v3 transition must move only `TFM-EXT-001..002` from the current effective tail
owner, the ledger must validate the cumulative v2-to-v3 chain, and a focused
source contract must pin optional-zero, mandatory-repeat, field-order, and
whole-`ne` semantics. Parameter rules and every integration surface remain
blocked through a dedicated extensible closure review. The review identities
and digests are recorded in `docs/evidence/tex-tfm-kern-pro-closure-v1.md`.

The immutable `tfm-validation-rule-transition-v3.json` now establishes that
ownership boundary without changing v1 or v2. Its raw SHA-256 is
`5929817fa92f3f8ead2a05ba33476281bb16ab5661eef5926730fe6fa27ce09d` and its
canonical SHA-256 is
`3206379d5f6f6748c2d532da83df565a187aee2077e936a67672336d10569ccf`.
It adds only `ExtensibleCheckedTfm`, moves exactly `TFM-EXT-001..002`, and maps
the optional-part and repeat predicates separately. The machine ledger now
validates the ordered cumulative v2-to-v3 chain, including omission, reorder,
wrong-effective-owner, duplicate, repeated-move, and predecessor-pin mutants.
The derived active counts are LigKern 8, Kern 1, Extensible 2, and Tail 3.
Production extensible symbols remain absent until focused source pins and RED
evidence exist.

The immutable `tfm-extensible-source-contract-v1.json` now pins both v3
ownership and the kern input contract. Its raw SHA-256 is
`5ce088a9e04d5de598fbabd4d59347f0e7c089f7cb491ebffe83314d3fc9ebdd` and its
canonical SHA-256 is
`e64c6d3d5afbf0349cab44eb22e57d0dc799786dbeddbc6c09c33e0f07dcb125`.
Official existence lines 11150..11154 retain SHA-256
`50b7893997fe98c90314983b83456c0fa15f577d02e91e2a03cf2a8034765c63`; the
complete recipe loop lines 11176..11183 has SHA-256
`c155058da84f06e687bd1cf226e3fc9900280abb1e4e60783360cb31f8f0c7cc`.
The contract distinguishes optional zero top/middle/bottom from mandatory
repeat zero, requires whole-`ne` iteration, excludes parameters and suffix, and
derives the exact successful maximum `ne=32753`. The focused interpretation is
`docs/tex82-read-font-info-extensibles.md`.

The strict-TDD private `ExtensibleCheckedTfm` implementation now consumes and
retains the exact kern state, validates every recipe in top/middle/bottom/repeat
order, and stores typed recipes. Evidence distinguishes optional-zero bypass
from mandatory repeat zero, fixes first recipe/field precedence, rejects an
unreferenced invalid recipe, processes the successful `ne=32753` maximum, and
rejects the absolute declared `ne=32755` geometry at its first mandatory repeat
without a panic. Parameter and suffix mutations do not affect recipe results;
the same raw allocation and all predecessor state remain retained. All eight
extensible-owned native witnesses have exact runtime errors and every
parameter-owned witness passes through. The RED record is
`docs/evidence/tex-tfm-extensible-tdd-red-v1.md`. The state remains root-private
and uncalled pending an extensible closure review.

This proves that the current crate is a bounded dimension-subset extractor, not
a full TFM validity oracle. It already rejects the invalid selected
`fontdimen5`, but it does not inspect several unrelated tables that native font
loading rejects; conversely, its exact-frame policy rejects the
native-accepted trailing word. The 2026-08-26 Pro review selected the narrow API
instead of widening this bounded unit into a purported complete validator.

The API-confusion part of `ARCH-016` is mitigated: call sites must spell
`dimension_subset::extract_exact_frame`, the error/hash names declare the frame
scope, broad aliases are absent, and a native-invalid subset-success witness is
executable. However, complete font-load validation remains open. Before any
source-visible font definition, resolver, cache, or owner commits state, a
separately reviewed validator must derive every rule from the pinned TeX82/
pdfTeX loader source and return one opaque artifact that binds immutable input
bytes, full validation evidence, declared-frame extent, raw content identity,
and extracted fields. A loader must not validate one byte stream and extract or
reacquire another. `ARCH-016` and W3 therefore remain blocked for source
loading even though the subset API is now honest.

## Owner, persistence, and capability decisions

MAG1 implements the reviewed owner contract as follows:

- `RequestedMagnification` stores any `i32`. Its virtual root default is 1000,
  local 1000 remains materialized, and ordinary Eqtb/SaveStack local/global
  assignment semantics own its layers.
- `PreparedMagnification` can only contain `1..=32768`. The latch is separate
  from `EqKey`, is not grouped, and assignment never mutates it. Preparation
  installs the first legal request; an illegal first request is globally
  corrected to 1000, while a later incompatible request is globally corrected
  to the already-prepared value.
- Optional `VmMagnificationStateV1` stores one requested owner per canonical
  scope layer plus the prepared effective value. Empty present state, root
  `Some(1000)`, layer-count mismatches, and illegal prepared values fail before
  VM/interner mutation. Requested/prepared mismatch is valid and round-trips.
- `state.magnification.v1` is derived only from family presence. The semantic
  fingerprint uses the V3 domain and `vm.magnification-state.v1`, and
  distinguishes owner layer, requested value, and latch value. V3 also tags
  optional mathcode and delcode DTOs with distinct family frames. Absent state
  preserves exact legacy bytes and hashes; dimension-only state preserves its
  frozen V2 identity; `LegacyOnly` rejects present magnification state instead
  of dropping it.
- The implementation is split across `f158376` (owner), `075650a` (reader and
  atomic restore), and `129894a` (hash, replay rekey, and writer refusal).
  Review remediation `c1db9f8` adds family-tagged V3 identity and `e4f917c`
  hardens restore and owner-branch evidence. Source activation remains a
  separate gate.

Implementation review `6a8dc94a-1e08-83ee-bf6c-0ca00a8b68d6` returned
`REVISE_MAG1` at 0.97 confidence after finding that frozen V1/V2 code-table
framing could alias identical mathcode and delcode DTOs when dormant tokens kept
both capabilities present. MAG1's V3 remediation closes that path for every
magnification-bearing checkpoint and freezes literal family-distinct hashes and
replay substitution rejection. Existing generic V1/V2 artifacts are not
silently rekeyed; their separate migration question is recorded as `ARCH-017`.
Closure review `6a8dd287-ca6c-83e8-abf2-4ca5827a8d97` returned
`APPROVE_MAG1` at 0.96 confidence. It accepted the family-distinct V3 framing,
consumer-level replay rejection, and rootless preflight atomicity evidence.
Composite all-family V3 vectors, independently restorable substitution proof,
a unique token-register control-sequence interner witness landed in `d8d914e`.
Reason-coded replay metrics remain a production versioned-writer observability
gate, not a MAG1 closure blocker. Exact closure gates pass 75 tex-checkpoint
library tests and every target, the 5-test MAG1 restore contract, 239 latexd
library tests, canonical workspace Clippy, rustfmt, and diff checks.

The current decisions and unresolved gates are:

| Question | W2.5-SC0 decision |
| --- | --- |
| What owns current-font identity? | No production owner is selected. A future reviewed typed owner must preserve definition identity, effective scale, grouping, and restore. |
| Who resolves metrics? | An owner/service outside the scanner. The scanner receives resolved `quad_sp`, `x_height_sp`, and provenance only. |
| Missing metric behavior? | No fallback. Exact VM diagnostic and recovery behavior remains unresolved and blocks W3. |
| What owns magnification? | The readiness review selected a dormant owner with grouped arbitrary requested integers and a non-grouped prepared value in `1..=32768`; native transitions are frozen by the 26-case fixture. Source activation remains forbidden. |
| Source-visible before epoch 6? | Neither font selection nor `\mag` may become newly source-visible at epoch 5. |
| Snapshot state? | MAG1 adds optional `VmMagnificationStateV1`, `state.magnification.v1`, and semantic-hash framing while preserving absent-state legacy identity and making `LegacyOnly` refuse present state. Current-font state remains unresolved. |
| Source-unreachable prerequisite state? | The dormant magnification owner and persistence unit passed mandatory implementation and closure review. Current-font state is not authorized. |
| Wider atomic W3? | Not selected. A later review must choose widened W3, dormant prerequisites, or a revised epoch/capability sequence. |
| Executable behavior identity? | Passive `primitive.dimension-parameter-command.v1` is identity/owner linkage only. Full execution needs an explicit reviewed behavior-capability decision. |
| Layout behavior? | Storage/query execution and paragraph-layout consumption remain separate capabilities and epochs. |

The remaining current-font and source-activation choices are intentional gate
outputs, not implicit defaults. Dormant MAG1 implementation does not authorize
production source behavior.

## Atomicity and non-activation matrix

| Semantic epoch | Executable `\hangindent` | Allowed |
| --- | --- | --- |
| 5 | No | Yes; current W2/W2.5 state. |
| 5 | Yes or partial | No. |
| 6 | No | No. |
| 6 | pt/sp-only under the current identity | No. |
| 6 | Full separately reviewed command | Future W3 target only. |

W2.5-SC0 also keeps the current generic `\dimen` scanner and arithmetic
unchanged. A future shared scanner must carry separate policies and prove both
the frozen register behavior and the native dimension-parameter behavior; the
existing register path cannot be reused unchanged.

## Readiness and failure gate

W3 remains blocked while any of these conditions is true:

- current-font identity, grouping, or restore is unspecified;
- production current-font definition/effective scale is not bound to exact TFM
  content identity;
- missing metrics lack exact diagnostics and token progress;
- a source-visible magnification transition lacks a separately reviewed source
  epoch and atomic activation contract;
- a prerequisite would become source-visible at epoch 5;
- implementation requires a cmr10 or magnification fallback;
- pt/sp-only execution is proposed under the current activation identity;
- generic `\dimen` behavior would change;
- command execution could be present without the atomic epoch-6 transition.

MAG1's dormant owner, persistence contract, and V3 remediation passed closure
review. A later plan must still choose the current-font owner and the atomic
source-visible epoch transition. Until then W3 remains blocked.
