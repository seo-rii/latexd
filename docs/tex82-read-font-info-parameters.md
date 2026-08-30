# TeX82 TFM Parameter Validation Boundary

The compatibility authority is the official TeX82 source at
`https://tug.ctan.org/systems/knuth/dist/tex/tex.web`, whose complete SHA-256 is
`c62ab513ef167e93f71a23bd34f311e243210afd7c7a0f9b779614b71e398324`.
Immutable `tfm-parameter-source-contract-v1.json` has raw SHA-256
`223aad57857393d02096adbdaa9cc587be13c515e9e7e86e1b19454f0c8164dd`
and canonical SHA-256
`90983c5403e96dacbf16767a5cb343ca91c7913d7e60925a497b38870ab36265`.
It pins ownership transition v4 and the extensible input source contract by
their raw and canonical identities.

## Exact source ranges

The contract pins these unmodified source byte ranges, including their trailing
newlines:

| Purpose | TeX82 range | SHA-256 |
| --- | --- | --- |
| Shared `store_scaled` implementation | lines 11108..11130 | `306907b8734bfa4dc990546e1fb84d0158c2b9af2338faed18808a06c4bfa58e` |
| Complete parameter source context | lines 11188..11199 | `3ab5b795c1f4f0f3f28883d345d4264a3a6d8c5ed391bb41ac23456df5027c07` |
| Signed slant branch | lines 11189..11195 | `150a57332ca1d79eac34af2a76283536424a51cfc7b4fdcd264ec210de01903f` |
| Non-slant `store_scaled` branch | lines 11196..11196 | `b281fc02beafc4e18958430f3525d61205b5006dd5bf6b712e46d3bd9520f134` |
| Contextual EOF check | lines 11197..11197 | `a33f2363a60c6b862002eb890c3d95e46f25f1e0672f01c4d226b48ef72c1da0` |
| Standard-parameter zero fill | lines 11198..11198 | `9d3fe901814da1c8bf0d8776a1891d590e83186b90b5978df9a0819edaf9f2bd` |

## Proof semantics

The phase consumes `ExtensibleCheckedTfm` by value and may produce only private
`ParameterCheckedTfm`. It owns exactly `TFM-PARAM-001..003`.

TeX iterates every declared parameter in the whole `np` loop. Parameter one is
the signed slant pure number: its signed 12.20 source value is converted to a
signed 16.16 stored value by discarding four low fractional bits, without
effective-size scaling. Parameters two through `np` use the already pinned
`store_scaled` algorithm and therefore depend on the same effective size as box
and kern words.

After the complete declared loop and the contextual EOF check, TeX fills only
missing standard slots through parameter seven with zero. Consequently `np=0`
creates seven zero slots, `np<7` preserves every declared value and fills the
rest, and `np>7` validates and retains every declared extra parameter. The
exact successful geometry limit is `np=32755`: the maximum declared frame is
32,767 words and the minimal valid non-parameter structure occupies 12 words.

The source range pins the EOF line so ordering cannot drift, but this private
phase does not own or reread EOF state; predecessor frame proof already bounds
the parameter bytes. Raw suffix bytes and final font adjustments are excluded
as well. Completion, callers, loading, cache, serialization, persistence, VM,
checkpoint, and public API remain blocked. Production symbols may be added only
after prospective RED evidence, and the resulting private phase requires a
dedicated parameter closure review.
