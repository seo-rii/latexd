# M13.3-DP1 `\hangindent` Characterization

Status: characterization complete; runtime/schema/source activation not started.

This unit establishes the next bounded M13.3 assignment owner without changing
production behavior. `\hangindent` is still absent from builtin lookup, Eqtb,
the command model, snapshot capabilities, checkpoint state, and rendering. The
checkpoint VM semantic epoch remains 5.

## Why this owner

`\hangindent` has a fresh INITEX default of 0sp, uses the ordinary dimension
scalar already represented by `EqValue::Dimension(i32)`, and has no current
latexd rendering or source-recovery consumer. `\hsize` and `\parindent` already
occur in source heuristics, while box and font state require identity and
ownership designs larger than one scalar assignment slice.

The plan review `6a807f92-8c54-83e8-8e47-21f683454768` approved only this
characterization gate. It requires separate later commits for passive wire
contracts, the dormant in-memory owner, state persistence/hash promotion, and
source activation with epoch 6.

## Authoritative oracle

[`scripts/check_hangindent_oracle.py`](../scripts/check_hangindent_oracle.py)
runs every case twice in independent `pdftex -ini -interaction=nonstopmode`
processes: once with `\hangindent` and once with `\dimen0`. The 14-case matrix
covers defaults, optional equals and repeated signs, fractional-sp rounding,
all TeX82 physical units, cmr10 `em`/`ex`, true units at `\mag=2000`, internal
dimensions, `\the`/`\number`/`\ifdim`, grouping and `\globaldefs`, arithmetic,
odd signed division, `\afterassignment`, aliases, local shadowing, dynamic
lookup, malformed input, maximum dimensions, overflow, and divide by zero.

Error-sensitive cases run in fresh processes and preserve the final value,
diagnostic sequence, exit status, and a trailing sentinel. The CI
`hangindent-oracle` artifact also records the resolved engine path, full
version, executable SHA-256, exact source and source SHA-256 for every process,
raw combined output, invocation, locale, timezone, and normalization rules.
The two native owners match in every normalized case.

Notable native facts are:

- the default is 0sp and direct assignment is bounded at 1,073,741,823sp;
- `.5sp` and `-.5sp` both become 0sp;
- cmr10 `1em` is 655361sp and `1ex` is 282168sp in this INITEX probe;
- `1truept` at `\mag=2000` is 32768sp;
- odd signed division truncates toward zero;
- repeated `\advance` can wrap the signed 32-bit scaled-point value;
- an oversized direct dimension stores the maximum and fires
  `\afterassignment` after the diagnostic;
- failed multiplication and division by zero preserve the previous value;
- `\dimexpr` is undefined in the TeX82 `pdftex -ini` target and is excluded
  from DP1 v1.

## Sharing decision

| Behavior | Native `\hangindent` | Native `\dimen0` | Current VM `\dimen0` | DP1 decision |
| --- | --- | --- | --- | --- |
| Default/scalar | 0sp, signed scaled points | Same | 0sp, `i32` scaled points | Reuse `EqValue::Dimension(i32)` with a distinct key and parameter range validation. |
| Optional `=` and signs | `=` optional; repeated signs accepted | Same | Assignment currently requires `=` and its literal scanner consumes one sign | Parameter-local scanner policy required. |
| Fractional sp | `.5sp` truncates to 0sp | Same | `.5sp` rounds to 1sp | Do not reuse the register literal conversion unchanged. |
| Physical units | `pt`, `sp`, `in`, `pc`, `cm`, `mm`, `bp`, `dd`, `cc` | Same | Literal register scanner accepts only `pt` and `sp` | Activation is blocked until v1 either implements the native set locally or explicitly narrows the contract in a reviewed gate. |
| Relative/true units | `em`/`ex` read current font; `true*` reads `\mag` | Same | Register scanner has no font-relative or true-unit context | Requires an explicit current-font/`\mag` policy before activation. |
| Internal dimension | Accepted | Accepted | Accepted for dimension/skip registers and simple macros | Share only the proven internal-value branch through a typed lvalue. |
| Query/conditional | `\the`, `\number`, and `\ifdim` work | Same | `\the`/`\ifdim` exist, but `\number\dimen0` does not yield the native scaled value | Parameter-local query dispatch required. |
| `\dimexpr` | Undefined in this TeX82 target | Same | No builtin | Explicitly unsupported in DP1 v1. |
| Group/global state | Local unwind, global cancellation, both `\globaldefs` polarities | Same | Register Eqtb/SaveStack behavior exists | Reuse common ownership only after the dormant-owner reference model passes. |
| Arithmetic | Advance/multiply/divide; signed odd division truncates; checked failure retains the old value | Same | Advance uses host addition, multiply saturates, divide-by-zero returns silently | Parameter-local arithmetic/error policy required; do not alter register behavior. |
| Recovery/hooks | TeX diagnostics, defined token progress, `\afterassignment` after scanner errors | Same | Several scanner failures return silently and do not share the native hook sequence | Parameter-local recovery tests are mandatory before source activation. |
| Layout effect | Native TeX consumes it in paragraph layout | Same scalar owner | No `hangindent` owner or consumer exists | DP1 command v1 will deliberately remain storage/query-only; future layout consumption needs a new behavior capability and epoch. |

The current VM characterization also freezes its existing `\dimen0` behavior:
`1.5pt` becomes 98304sp, `.5sp` rounds to 1sp, and `-5sp/2` truncates to -2sp.
These are observations, not permission to change the existing register path.

## Next gate

Before production code changes, Gate 1 must review and freeze both passive
capability contracts:

- `eqtb.dimension-parameter-state.v1`, whose v1 ID allowlist is exactly
  `hangindent`;
- `primitive.dimension-parameter-command.v1`, still non-executable and
  non-writable at the passive-reader stage.

The gate must resolve the physical/current-font/true-unit contract and exact
parameter-local arithmetic/recovery policy. Characterization failure is a
valid reason to stop; it must not be hidden by changing existing dimension
register semantics.
