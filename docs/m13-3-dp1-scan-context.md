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
library tests, the full 80-test Python policy suite, package Clippy, canonical
workspace Clippy, rustfmt, and diff checks pass. Existing IntegerParameter V1
and epoch/capability contracts remain unchanged.

This crate closes metric representation and conversion, not production
provenance. A later owner must still bind a logical font definition and
effective size to the TFM hash and restore that state deterministically.

## Full TFM validity gate

[`scripts/check_tfm_validity_oracle.py`](../scripts/check_tfm_validity_oracle.py)
runs 54 byte-frozen mutations in separate pdfTeX INITEX processes. Its expected
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
Two final 54-process reports are byte-identical with SHA-256
`b3d764d04cb4dce9f64aa57a13441db92739fe3babe804843f5b8baef7d6f3d9`.

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

Phase 4 begins that closure with a per-case natural/explicit-at schema, valid
cmr10 controls at 1sp and 16sp, and paired width/height/depth/italic entry-zero
mutations. Each pair has identical TFM bytes: all four load at 1sp and reject at
16sp with the exact bad-TFM recovery. Positive state, EOF/trailing-suffix,
empty-range, absent-character, extended-parameter, and source-ledger evidence
remain open before another readiness review.

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
