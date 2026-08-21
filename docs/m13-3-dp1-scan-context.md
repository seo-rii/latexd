# M13.3-DP1 Dimension Scan-Context Gate

## Status and boundary

Pro readiness review `6a8862e6-12a4-83e8-94ab-2cd2088661bd` returned
`REVISE_PLAN` with confidence 0.96. W3 remains blocked. The approved next unit,
W2.5-SC0, characterizes the current-font and magnification context required by
dimension scanning without changing production semantics.

Two W2.5 implementation-review submissions on 2026-08-22 ended before chat
creation when the shared browser broker exited during startup (`Browser.close:
Connection closed while reading from the driver`, then signal 15/broken pipe).
The live browser/authentication preflight passed between the attempts. No
implementation-review UUID or verdict was produced, so the failures are
recorded as review-infrastructure unavailability, not approval or rejection.

This unit adds no source command, primitive, Eqtb owner, dependency, snapshot
field, capability, semantic hash frame, writer lane, or layout consumer.
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
runs each of 22 cases in a fresh `pdftex -ini -interaction=nonstopmode`
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

## Owner, persistence, and capability decisions

W2.5-SC0 records the following decisions and unresolved gates:

| Question | W2.5-SC0 decision |
| --- | --- |
| What owns current-font identity? | No production owner is selected. A future reviewed typed owner must preserve definition identity, effective scale, grouping, and restore. |
| Who resolves metrics? | An owner/service outside the scanner. The scanner receives resolved `quad_sp`, `x_height_sp`, and provenance only. |
| Missing metric behavior? | No fallback. Exact VM diagnostic and recovery behavior remains unresolved and blocks W3. |
| What owns magnification? | No production owner is selected. Native default/query/scope/recovery are frozen by the fixture. |
| Source-visible before epoch 6? | Neither font selection nor `\mag` may become newly source-visible at epoch 5. |
| Snapshot state? | Both contexts affect later scanning and therefore must be restored deterministically before becoming source-visible. Their schema and capability are unresolved and block W3. |
| Source-unreachable prerequisite state? | Possible only after a separate reviewed persistence/hash contract; not authorized here. |
| Wider atomic W3? | Not selected. A later review must choose widened W3, dormant prerequisites, or a revised epoch/capability sequence. |
| Executable behavior identity? | Passive `primitive.dimension-parameter-command.v1` is identity/owner linkage only. Full execution needs an explicit reviewed behavior-capability decision. |
| Layout behavior? | Storage/query execution and paragraph-layout consumption remain separate capabilities and epochs. |

These unresolved ownership and persistence choices are intentional gate
outputs, not implicit defaults. Successful characterization does not authorize
production implementation.

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
- quad/x-height provenance is not deterministic;
- missing metrics lack exact diagnostics and token progress;
- magnification owner, persistence, or source epoch is unspecified;
- a prerequisite would become source-visible at epoch 5;
- implementation requires a cmr10 or magnification fallback;
- pt/sp-only execution is proposed under the current activation identity;
- generic `\dimen` behavior would change;
- command execution could be present without the atomic epoch-6 transition.

The next production plan must choose owners, snapshot/capability/hash treatment,
and epoch placement, then receive a new readiness review. Until then W3 remains
blocked.
