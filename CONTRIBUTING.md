# Contributing to latexd

`latexd` is an experimental compiler and preview stack. Small, test-backed
changes are easier to review than broad compatibility patches.

## Development Setup

Required tools:

- Rust and Cargo.
- Node.js and `pnpm` for the web workspace.
- Optional: Ghostscript for real raster rendering tests.
- Optional: `pdftotext` for PDF text oracle checks.

Useful commands:

```bash
cargo fmt --check
cargo test -q
pnpm -C web install
pnpm -C web test
```

## Patch Guidelines

- Keep compatibility fixes narrow and covered by a fixture or unit test.
- Prefer adding a reduced fixture over depending on a large external paper.
- Do not vendor full arXiv papers or PDFs into the repository.
- Keep renderer work separated from TeX execution semantics where possible.
- Preserve source spans and diagnostics when changing parser or VM behavior.
- Avoid replacing existing external-compiler behavior while the internal
  renderer is still incomplete.

## Test Layers

Use the smallest test layer that can catch the regression:

- unit tests for token, VM, aux, layout, and PDF primitives;
- checked-in fixtures for reduced document compatibility cases;
- ignored local oracle tests for downloaded arXiv source/PDF corpora;
- browser tests for preview, source sync, and WebSocket behavior.

The local arXiv oracle workflow is documented in
[`fixtures/arxiv-oracle/README.md`](./fixtures/arxiv-oracle/README.md).

## Reporting Issues

When reporting a compiler or rendering bug, include:

- the smallest LaTeX input that reproduces the issue;
- the compiler mode used, such as `internal` or an external compiler;
- expected behavior from a reference LaTeX toolchain when relevant;
- diagnostics, logs, or oracle report excerpts.

Avoid attaching copyrighted source archives directly unless their license allows
redistribution.
