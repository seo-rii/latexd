# latexd web workspace

This `pnpm` workspace contains the browser-side pieces for `latexd`.

## Packages

- [`apps/viewer/`](./apps/viewer): SvelteKit shell that mounts the viewer and owns the
  `latexd` HTTP/WebSocket adapter.
- [`packages/viewer-core/`](./packages/viewer-core): reusable vanilla TypeScript viewer
  runtime with injected transport.

## Common commands

```sh
pnpm install
pnpm build
pnpm test
pnpm check
pnpm clean
```

## Development flow

Start the Rust daemon separately, then run the frontend workspace in dev mode:

```sh
cargo run -p latexd -- serve --root fixtures/arxiv-basic --compiler-bin internal
pnpm dev
```

The SvelteKit app proxies `/api`, `/artifacts`, and `/ws` to
`http://127.0.0.1:4380` by default. Override that with `LATEXD_DEV_ORIGIN`
when you need a different daemon origin.

## Build output

`pnpm build` produces the static viewer app under [`apps/viewer/build/`](./apps/viewer/build).
The build compiles the lightweight binding in `crates/latexd-wasm` and the
memfs compiler process in `crates/latexd-wasi`. Install Rust's
`wasm32-unknown-unknown` and `wasm32-wasip1` targets plus `wasm-pack` before
building locally.
The Rust daemon serves that output by default, or a custom directory via
`LATEXD_VIEWER_DIST`.

## Browser-only mode

The deployed [GitHub Pages viewer](https://seorii.page/latexd/) falls back to a
local WASI compiler when no `latexd` daemon is available. Project directories
are mounted into an isolated in-memory `/workspace`; the WASI process runs the
same TeX VM, RenderEvent stream, Document IR builder, page builder, and PDF
writer as the native compiler. The browser UI accepts project directories,
supports multiple TeX/class/style/auxiliary sources, keeps binary graphics in
memfs, and exposes the generated PDF for download. External programs such as
Ghostscript, Poppler, and shell-escape commands remain native-only.

Run the browser-only integration test after a static build:

```sh
pnpm test:e2e:wasm
```
