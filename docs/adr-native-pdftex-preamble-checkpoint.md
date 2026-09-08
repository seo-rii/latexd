# ADR: Native pdfTeX format dumps as a bounded preamble experiment

Date: 2026-09-08. Status: evaluated candidate; production engine unchanged.

## Decision

Keep native pdfTeX as a compatibility oracle and candidate for a future
preamble adapter. A native `.fmt` file is a usable checkpoint for the measured
closed, side-effect-free preambles. Do not route production compilation through
this experiment, treat `.fmt` as an arbitrary continuation, or declare the engine
selection milestone complete from these fixtures.

The current internal mini-kernel remains explicitly approximate. Choosing a
replacement production core still requires the review's representative document
comparison, pinned package/font profile, state/effect boundaries, and deployment
assessment. The existing external adapter continues to preserve native PDF
layout, geometry, and resources in the common page-preview manifest.

## Reproducible evidence

Run [the probe](../scripts/probe_pdftex_format.py) from the repository root with
a fresh output directory:

```sh
python3 scripts/probe_pdftex_format.py \
  --output .codex/engine-poc/reproduction \
  --corpus .codex/corpus/cc0
```

The measured run is [run-2/report.json](../.codex/engine-poc/run-2/report.json).
It contains the fixture manifest, input hashes, executable hash, recorded file
paths and first observed hashes, commands, PIDs, exit statuses, timings, and
comparison results. Inputs and output effects live in separate directories for
the clean, dump, and restored processes. The original corpus directories are
hashed before and after copying and remain unchanged. This wrapper supplies the
immutable input boundary; pdfTeX itself does not create a project revision.

The available engine was pdfTeX 1.40.24, TeX Live 2022/Debian, with the installed
LaTeX kernel and packages. The probe records 222 system input files. This is an
observed local environment, not a frozen, distributable TeX Live 2025 profile.
Missing packages were not installed and original papers were not patched.

The native interface is documented in the installed primary manual
`/usr/share/man/man1/pdftex.1.gz`: `-ini` enters format-dump mode, `-fmt` selects
the format, `-recorder` records file paths, and `-no-shell-escape` disables
external commands. The installed `latex.ltx` also ends its initialization with
`\dump`. The probe uses these native facilities; it adds no checkpoint hooks to
pdfTeX. It holds `jobname=main` and the execution date constant across lanes.

## Results

The experiment comprises **20 controlled fixtures, not 20 papers**: 16 positive
preamble candidates and four state/effect boundary probes. Each successful
output runs three passes, then compares exact extracted text, all page pixels
at 72 DPI, and emitted `.aux`, `.toc`, `.out`, `.idx`, and `.effect` files. PDF
bytes are recorded but are not the equality oracle.

| Probe | Measured result |
| --- | --- |
| 16 supported fixture candidates | 16/16 match text, all 17 page rasters, and file effects |
| Classes/layout | `article`, `amsart`, two columns |
| Math/resources | AMS math, embedded graphics, T1 font encoding, tables |
| Semantic state | References, TOC, nested macros, catcodes, local/global groups, registers, included preamble, hyperref, theorem |
| Immediate preamble write | Dump and restore succeed; pixels match, output file effect is absent after restore |
| Open write stream | Dump and restore succeed; pixels match, body write does not reproduce the clean output file |
| Open group | Native dump rejects the group boundary |
| After shipout | Dump succeeds; isolated restore fails looking for `main.aux` |

The after-shipout result establishes a failure of this isolated format-only
recipe. It is not a claim that every possible engine fork or effect-restoring
adapter must fail. The experiment does not save the input stack, earlier PDF
pages, open streams, or a transactional journal and does not establish a safe
mid-document continuation API.

An additional dependency mutation experiment captures `defs.tex` containing
`ALPHA`, builds its format, and changes the live file to `BRAVO`. The captured
bytes remain unchanged. A clean new revision emits `BRAVO`; reusing the old
format against the new directory emits `ALPHA`. All four assertions pass. A
future adapter must therefore validate the preamble readset and environment
before selecting a format. The native format loader does not invalidate changed
inputs on behalf of the caller.

Six existing CC0 papers were tested only through their literal preamble boundary:

| arXiv source | Dump result in installed TeX Live 2022 |
| --- | --- |
| 2404.05196v2 | Success; full-paper restoration was not tested |
| 2602.14379v1 | Missing `algorithm.sty` |
| 2508.10038v1 | Missing `algorithm2e.sty` |
| 2403.07956v1 | Missing `algorithm.sty` |
| 2102.03748v1 | Missing `algorithmicx.sty` |
| 2302.01837v1 | Missing `revtex4-2.cls` |

These five failures describe the local reference environment, not latexd
compatibility or intrinsic format-dump limitations. One successful preamble is
not a successfully rendered paper and is not counted as corpus coverage.

## Limits and next gate

The final run exited 0 in 41.26 seconds, issued 231 bounded subprocesses, and
produced approximately 349 MB before the report. Positive fixture formats were
6.48–6.63 MB each. Each subprocess has a 25-second wall timeout, 20-second CPU
limit, 1 GiB address-space limit, and 32 MiB file limit; controlled fixture output
has a 500 MiB budget. Shell escape and automatic font/format generation are off.
These are experiment limits, not a production sandbox contract.

The final run's median native pass was 217.4 ms clean and 167.8 ms restored, with
a 518.9 ms median dump. An earlier run on the shared machine measured 167.7 ms
clean and 167.5 ms restored. These small sequential samples do not establish a
speedup, p95 latency, edit-to-visible response, or process-startup-free reuse.

The next native gate is to freeze the target profile, run full clean/restored
documents, and define admissible preamble effects with explicit invalidation and
transactional replay. Only then should an adapter expose prefix reuse. Tail
convergence and arbitrary shipout checkpoints require separate engine-level
work and differential validation; native format files do not close those tasks.
