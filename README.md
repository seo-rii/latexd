# latexd

`latexd` is a Rust workspace for an incremental LaTeX build and browser-preview pipeline.
The current implementation targets an arXiv-like `pdflatex`-first workflow, keeps the last
good preview on failures, and connects page/tile refresh with source-preview sync and local
editor bridge surfaces.

This README is intentionally short. The long design and planning material that used to live
here now lives under [`docs/`](./docs).

## Current Status

- Milestones `M0` through `M12` are treated as complete by the repository's current definition
  of done.
- Current work is post-`M12` follow-on hardening, not an open `M12` blocker.
- The latest milestone/progress view lives in [`PROGRESS.md`](./PROGRESS.md).

## What The Project Does

- Builds and previews LaTeX documents through a Rust workspace centered on `latexd`.
- Preserves stable page identity and supports page-level or tile-level preview refresh.
- Exposes source jump, hover, open-source, hash deep-link, and in-browser source-pane flows.
- Tracks semantic aux state, replay checkpoints, page reuse, and structured build metadata.
- Uses a realistic fixture corpus to pin regression behavior across wrapper-heavy arXiv-style
  projects.

## Repository Layout

- [`crates/`](./crates): Rust crates for protocol, VM, layout, PDF, checkpointing, renderer,
  semantic aux, and the `latexd` daemon itself.
- [`web/viewer/`](./web/viewer): browser viewer runtime and viewer regression suite.
- [`fixtures/`](./fixtures): `arxiv-basic` and `arxiv-smoke` fixture corpora.
- [`docs/`](./docs): architecture, roadmap, protocol, testing, backlog, and milestone detail.

## Requirements

- Rust toolchain with Cargo.
- Node.js for the browser viewer regression suite.
- Ghostscript only if you want the real tile renderer instead of the default mock path.

## Quick Start

Run the daemon against the bundled sample project:

```bash
cargo run -p latexd -- serve --root fixtures/arxiv-basic
```

Then open `http://127.0.0.1:4380/` in a browser.

Useful local commands:

```bash
cargo run -p latexd -- --help
cargo test -q
node web/viewer/app.test.mjs
```

## Documentation

- Architecture: [`docs/architecture.md`](./docs/architecture.md)
- Roadmap: [`docs/roadmap.md`](./docs/roadmap.md)
- HMR protocol: [`docs/hmr-protocol.md`](./docs/hmr-protocol.md)
- Testing strategy: [`docs/testing-strategy.md`](./docs/testing-strategy.md)
- Contributor notes: [`docs/contributor-notes.md`](./docs/contributor-notes.md)
- Progress snapshot: [`PROGRESS.md`](./PROGRESS.md)
- Work backlog: [`docs/work-backlog.md`](./docs/work-backlog.md)
- `M12` definition of done: [`docs/m12-checklist.md`](./docs/m12-checklist.md)
- Renderer/session follow-up: [`docs/renderer-session-plan.md`](./docs/renderer-session-plan.md)
