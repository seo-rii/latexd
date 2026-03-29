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
The Rust daemon serves that output by default, or a custom directory via
`LATEXD_VIEWER_DIST`.
