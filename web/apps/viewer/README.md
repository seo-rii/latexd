# @latexd/viewer-app

`@latexd/viewer-app` is the SvelteKit shell for the embedded `latexd` browser viewer.

## Responsibilities

- Mounts `@latexd/viewer-core` inside a SvelteKit route.
- Owns the latexd-specific HTTP and WebSocket transport adapter.
- Produces the static frontend build served by the Rust daemon.
- Keeps app-level assets and shell concerns separate from the reusable viewer runtime.

## Commands

```sh
pnpm -C ../../ dev
pnpm -C ../../ check
pnpm -C ../../ build
```

For local SvelteKit development, run `latexd` separately and point the dev proxy at it with
`LATEXD_DEV_ORIGIN` if you are not using `http://127.0.0.1:4380`.
