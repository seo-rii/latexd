# latexd

`latexd` is an experimental Rust workspace for incremental LaTeX compilation,
PDF preview, and browser-based source sync.

The project started as an arXiv-style preview pipeline: keep the last good
preview visible, rebuild only when inputs change, and expose enough structured
metadata for page reuse, source jumps, diagnostics, and future internal
rendering work.

## Status

`latexd` is usable as a development prototype, not yet as a drop-in LaTeX
engine. The external-compiler preview path is the most practical path today.
The internal compiler can process a growing subset of arXiv-like projects, but
its PDF output is still a text scaffold rather than full TeX-quality rendering.

Current focus:

- incremental build and preview infrastructure;
- arXiv-style project resolution and fixture coverage;
- semantic aux data for labels, citations, bibliography, and source sync;
- internal compiler smoke coverage against local oracle corpora;
- the next rendering architecture: event stream, Document IR, page layout, and
  renderer backends.

See [`PROGRESS.md`](./PROGRESS.md), [`PLAN.md`](./PLAN.md), and
[`docs/real-rendering-plan.md`](./docs/real-rendering-plan.md) for the current
engineering plan.

## Features

- Rust daemon for watching LaTeX projects and serving preview artifacts.
- Browser preview shell with WebSocket updates and source-preview bridge flows.
- External compiler support for realistic `pdflatex`-style workflows.
- Internal TeX lexer, token model, VM, mini LaTeX bootstrap, semantic aux scan,
  simple layout, and PDF text output.
- Checkpoint and page-reuse infrastructure for incremental rebuild work.
- Fixture suites for small documents, synthetic arXiv-style projects, and local
  arXiv oracle smoke tests.

## Repository Layout

- [`crates/latexd`](./crates/latexd): daemon, compiler orchestration, tests.
- [`crates/tex-lexer`](./crates/tex-lexer): TeX tokenization.
- [`crates/tex-vm`](./crates/tex-vm): macro expansion and execution core.
- [`crates/tex-bootstrap`](./crates/tex-bootstrap): mini LaTeX bootstrap layer.
- [`crates/tex-aux`](./crates/tex-aux): semantic aux and bibliography scanning.
- [`crates/tex-layout`](./crates/tex-layout): current text-layout scaffold.
- [`crates/tex-pdf`](./crates/tex-pdf): current minimal PDF writer.
- [`crates/tex-render-gs`](./crates/tex-render-gs): Ghostscript-backed raster
  renderer integration.
- [`web/`](./web): frontend `pnpm` workspace.
- [`fixtures/`](./fixtures): checked-in compatibility and smoke fixtures.
- [`docs/`](./docs): architecture, roadmap, test strategy, and design notes.

## Requirements

- Rust toolchain with Cargo.
- Node.js and `pnpm` for the web preview workspace.
- Ghostscript if you want the real raster/tile renderer path.
- Optional: `pdftotext` for PDF text oracle checks.

## Quick Start

Install frontend dependencies:

```bash
pnpm -C web install
```

Build the frontend:

```bash
pnpm -C web build
```

Run the daemon against the bundled sample project:

```bash
cargo run -p latexd -- serve --root fixtures/arxiv-basic --compiler-bin internal
```

Then open:

```text
http://127.0.0.1:4380/
```

Useful development commands:

```bash
cargo test -q
cargo run -p latexd -- --help
pnpm -C web test
pnpm -C web dev
```

When developing the SvelteKit app directly, `pnpm -C web dev` proxies `/api`,
`/artifacts`, and `/ws` to `http://127.0.0.1:4380` by default. Override that
with `LATEXD_DEV_ORIGIN` if needed.

## arXiv Oracle Smoke Tests

The repository does not vendor full arXiv source archives or PDFs. Instead,
`fixtures/arxiv-oracle/cc0-smoke.json` describes a small local corpus, and
`scripts/fetch_arxiv_cc0_corpus.py` can download it into a separate local
directory.

Example:

```bash
python3 scripts/fetch_arxiv_cc0_corpus.py --output /tmp/latexd-arxiv-cc0
LATEXD_ARXIV_CC0_CORPUS=/tmp/latexd-arxiv-cc0 \
  cargo test -p latexd --test arxiv_oracle -- --ignored --nocapture
```

Use `LATEXD_ARXIV_ORACLE_STRICT=1` when you want the test to fail on oracle
threshold regressions. The oracle writes `cc0-smoke-report.json` plus per-case
`*-oracle.txt`, `*-internal.txt`, and `*-internal.pdf` artifacts under
`$LATEXD_ARXIV_CC0_CORPUS/reports` by default; override this with
`LATEXD_ARXIV_ORACLE_REPORT_DIR`.

## Documentation

- Architecture: [`docs/architecture.md`](./docs/architecture.md)
- Roadmap: [`docs/roadmap.md`](./docs/roadmap.md)
- Testing strategy: [`docs/testing-strategy.md`](./docs/testing-strategy.md)
- Real rendering plan: [`docs/real-rendering-plan.md`](./docs/real-rendering-plan.md)
- Real rendering accepted structure: [`docs/real-rendering-accepted-structure.md`](./docs/real-rendering-accepted-structure.md)
- Rendering design question: [`docs/real-rendering-design-question.md`](./docs/real-rendering-design-question.md)
- HMR protocol: [`docs/hmr-protocol.md`](./docs/hmr-protocol.md)
- Contributor notes: [`docs/contributor-notes.md`](./docs/contributor-notes.md)
- Frontend guide: [`web/README.md`](./web/README.md)

## Contributing

Contributions are welcome, but the project is still architecture-heavy. Before
large changes, read [`CONTRIBUTING.md`](./CONTRIBUTING.md) and prefer small,
test-backed patches.

Good first areas:

- focused regression fixtures;
- diagnostics and error classification;
- documentation cleanup;
- small internal compiler compatibility fixes;
- tests around source sync and semantic aux behavior.

## License

`latexd` is licensed under the [MIT License](./LICENSE).
