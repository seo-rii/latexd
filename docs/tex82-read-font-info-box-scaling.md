# TeX82 `read_font_info` box-scaling contract

## Authority and source pin

The compatibility authority is TeX82 `tex.web` from
`https://tug.ctan.org/systems/knuth/dist/tex/tex.web`. The audited full file has
SHA-256
`c62ab513ef167e93f71a23bd34f311e243210afd7c7a0f9b779614b71e398324`.
The focused lines 11108 through 11148, beginning with the fix-word definition
and ending after the `alpha`/`beta` normalization, have SHA-256
`fe78fd68fc804b9a567500e5c403865bf9e945b0e4e930f9de6bdc5813a68462`.
Both pins are recorded in the native oracle and its frozen fixture.

The relevant source order is exact:

1. reduce the already-bound effective size while it is at least `2^23`,
   doubling `alpha` each time;
2. compute `beta = 256 / alpha`, then replace `alpha` by the reduced size
   multiplied by it;
3. run `store_scaled` continuously from `width_base[f]` through the word before
   `lig_kern_base[f]`, thereby scaling width/height/depth/italic in that order;
4. allow only sign byte 0 or 255 and apply the source's nested integer divisions;
5. only after every word is scaled, check entry zero of width, height, depth,
   and italic in that order.

This ordering makes a later invalid sign win over an earlier nonzero scaled
entry zero, exactly as in the loader. Entry-zero validity is bound to the
effective size: the same raw word may scale to `0sp` at 1sp and become nonzero
at 16sp.

## Private implementation boundary

The root-private `check_boxes(CharacterCheckedTfm)` consumes the exact
predecessor and returns a private `BoxCheckedTfm`. It retains the predecessor
and typed scaled width/height/depth/italic arrays. It accepts no replacement
bytes, size, range, or identity input. The state is not `Clone`, has no public
or crate-visible path, and has no production caller.

`BoxValidationRule` distinguishes the exact table/index/sign failure from the
exact table/scaled-sp entry-zero failure. The machine rule ledger separately
pins exact `BoxCheckedTfm` proof ownership to `TFM-BOX-001`, `TFM-BOX-002`, and
`TFM-BOX-003`.

The private test matrix covers literal normalization boundaries, every
forbidden sign byte in every box table, source-order precedence, all four
entry-zero rules at 1sp and 16sp, the full 83-case content-addressed corpus,
predecessor identity, later-table and suffix isolation, maximum legal table
geometry, and bounded generated sign-valid inputs.

## Native exact-sp oracle

`scripts/check_tfm_box_scaling_oracle.py` mutates the reviewed repository
`cmr10.tfm` into ten probe characters. Each character points to the same raw
fix word in all four metric tables. One fresh `pdftex -ini
-interaction=nonstopmode` process then observes all ten characters at one
effective size, so the complete matrix needs only 21 processes.

The frozen matrix contains 21 effective sizes × 10 fix words. Sizes cover 1,
2, 15, 16, 17, both sides of `2^16`, both sides of every normalization boundary
from `2^23` through `2^26`, and `2^27-1`. Fix words cover zero, the least
positive and negative fractions, both nested-division carry boundaries, the
values immediately below one and sixteen, positive one, negative one, and
negative sixteen.

Width is observed as the box width. Italic is observed directly from the
italic-correction kern with `\lastkern`. TeX boxes clamp negative height/depth
when deriving their maxima, so native negative height/depth observations are
explicitly expected to be `0sp`; width and italic retain the negative exact-sp
scaler result. Positive height/depth observations match width and italic.

The versioned fixture is
`crates/tex-tfm-metrics/tests/fixtures/tfm-box-scaling-oracle-v1.json`, whose
current file SHA-256 is
`287f3c33038b05279239f0836af5e03a306f4589d41127eb3aec2af88f051eb4`.
CI runs its policy tests, executes the native oracle after TeX installation,
uploads engine/source/TFM/matrix provenance, and does so before the Rust suite.

## Explicit exclusions

This evidence closes only private box-table scaling and entry-zero checks.
Kern scaling and all lig/kern remains blocked. Extensible recipes, parameters,
complete-validator publication, public or crate-visible APIs, production font
ownership/resolution/caching, source-visible loading, checkpoints, and W3 also
remain outside this unit. A new Pro closure review is required before any later
TFM table phase starts.

## Closure decision and successors

Box-closure Pro review `6a93a948-81a8-83ee-8173-a0a58dbe1a08` returned
`PROCEED_PRIVATE_TFM_LIGKERN` at confidence 0.95 and found no blocking defect
in this box state. Its two evidence guardrails are now enforced: the native
oracle checks the base TFM SHA-256 before any probe mutation, and the AST policy
requires exactly one production construction and one authorized return path
for every private proof state.

The next and only authorized state transition is from `BoxCheckedTfm` to
private `LigKernCheckedTfm`, validating lig/kern instructions and boundary
state in source order. It must not scale kern fix words. A dedicated
lig/kern-closure review is required before a separate private
`KernCheckedTfm` may consume that predecessor and perform exact kern scaling.
Extensible recipes and every public or production integration remain blocked.
