# TeX82 `read_font_info` lig/kern instruction contract

## Authority and focused source pins

The compatibility authority remains the official TeX82 `tex.web`, whose full
SHA-256 is
`c62ab513ef167e93f71a23bd34f311e243210afd7c7a0f9b779614b71e398324`.
The helper at lines 11150..11154 has SHA-256
`50b7893997fe98c90314983b83456c0fa15f577d02e91e2a03cf2a8034765c63`.
It defines existence as both an unsigned-byte range check and an existing
character record, rather than range membership alone. The instruction and
boundary block at lines 11156..11172 has SHA-256
`a105c3b6349d6ad4c15e37f3cc0d8b64670c14ffc3f79cdd827da05043d28c5d`.

These pins are machine-recorded by
`tfm-validation-rule-transition-v2.json`. That transition retains the reviewed
v1 contract byte-for-byte and changes only the owner of `TFM-KERN-001` from
`LigKernCheckedTfm` to the later `KernCheckedTfm`.

## Exact source order

The private instruction validator must preserve this order:

1. initialize the boundary-label sentinel and an absent boundary character;
2. when `nl=0`, skip the complete instruction loop and retain both sentinels;
3. otherwise decode every four-byte instruction in table order;
4. for `skip_byte > 128`, first range-check the 16-bit restart target, then set
   the boundary character only when `skip_byte=255` on the first instruction;
5. for an ordinary instruction, check the next character unless it equals the
   installed boundary character, then check either the ligature replacement's
   existence or the kern index, then check a nonterminal forward skip;
6. after the complete loop, derive the boundary label only when the final
   instruction has `skip_byte=255`.

The boundary character is therefore available to later ordinary instructions,
but a first-instruction marker is not itself an ordinary instruction. The
terminal boundary label uses the final instruction's already range-checked
restart target. No instruction branch reads or scales a kern fix word.

## Private successor boundary

The only authorized implementation consumes `BoxCheckedTfm` and returns one
private `LigKernCheckedTfm` containing the exact predecessor, typed decoded
instructions, optional boundary character, and optional boundary-program
start. It must not scale kern fix words. The implementation has no public or
crate-visible path and no production caller.

Kern scaling remains a distinct reviewed successor. Only after lig/kern
closure may a private `KernCheckedTfm` consume `LigKernCheckedTfm`. Extensible
recipes, parameters, complete validation, source-visible font loading,
production ownership, checkpoints, and W3 remain blocked.

## Implemented evidence

The private `LigKernCheckedTfm` implementation now consumes and retains the
exact `BoxCheckedTfm`, stores every decoded instruction, and retains the
optional boundary character and boundary-program start. Its exact error space
preserves instruction index and source predicate identity. The corpus bridge
fixes 83/83 persisted corpus phase outcomes and
8/8 exact lig/kern-owned rejections while requiring `TFM-KERN-001` to succeed
through this phase.

An independent source-order oracle covers 4,096 generated programs in addition
to the complete selected single-instruction matrix. The
32,753-instruction maximum proves the largest legal table geometry remains bounded. Mutated kern
words and raw suffixes preserve the instruction result, so
kern words remain unread and unscaled. The AST policy allows exactly one production construction
and authorized return path for the proof state and rejects alternate creation,
alias, macro, visibility, and clone paths. A dedicated Pro closure review is
still required before any `KernCheckedTfm` implementation begins.
