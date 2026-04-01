# Viewer HMR Protocol

This document describes the runtime message surface between the `latexd` daemon and the
browser viewer as it exists today.

The transport is intentionally split:

* WebSocket on `/ws` carries build lifecycle updates and viewer viewport hints.
* HTTP carries point requests such as source jump, open-source, syncmap fetch, source file
  fetch/save, and tile manifests.

## WebSocket Message Surface

Current server messages are defined in
[`crates/hmr-protocol/src/lib.rs`](../crates/hmr-protocol/src/lib.rs):

```rust
enum ServerMsg {
    BuildStarted {
        rev: RevId,
        changed_files: Vec<String>,
    },
    Diagnostics {
        rev: RevId,
        items: Vec<Diagnostic>,
    },
    FullPdfReady {
        rev: RevId,
        pdf_url: String,
        page_ids: Vec<String>,
        page_artifacts: Vec<PagePreviewArtifact>,
    },
    PatchPages {
        rev: RevId,
        ops: Vec<PagePatchOp>,
    },
    SourceSnapshot {
        rev: RevId,
        files: Vec<SourceSnapshotFile>,
    },
    BuildFinished {
        rev: RevId,
        success: bool,
    },
}
```

Current client messages:

```rust
enum ClientMsg {
    OpenDocument {
        doc: String,
    },
    ViewportChanged {
        zoom: f32,
        current_page: u32,
        scroll_top: f32,
        visible_pages: Vec<String>,
    },
}
```

`OpenDocument` is defined in the shared protocol crate but is not currently used by the
bundled viewer. The SvelteKit viewer actively sends `ViewportChanged` over `/ws`.

## Message Semantics

### `BuildStarted`

Sent when the daemon accepts a rebuild for a new revision. The viewer uses this to mark the
UI as building and surface the dirty input set.

### `Diagnostics`

Carries the current revision diagnostics. This is sent for both successful and failed builds.

### `PatchPages`

Optional incremental page patch set for the revision. This only describes structural page
changes such as replace/insert/delete operations for revision-stable `page_id`s.

### `FullPdfReady`

Sent after the daemon has materialized the full preview for the revision. This carries the
revision PDF URL plus per-page artifact URLs:

```rust
struct PagePreviewArtifact {
    page_id: String,
    pdf_url: String,
    svg_url: Option<String>,
}
```

The viewer uses `page_artifacts` as the canonical preview page list. `page_ids` is retained
as a compact identity list and fallback.

### `SourceSnapshot`

Sent after a successful build to stream the current source text snapshot into the viewer:

```rust
struct SourceSnapshotFile {
    file: String,
    content: String,
    line_count: usize,
}
```

The bundled SvelteKit editor uses this to update its in-browser source cache without polling
`/api/source-files/<rev>` after every applied revision.

### `BuildFinished`

Marks the end of the revision build. `success = false` means the previous successful preview
stays mounted while diagnostics update.

## Typical Successful Revision Flow

For a successful build, the daemon emits messages in this order:

1. `BuildStarted`
2. `Diagnostics`
3. `PatchPages` when structural page changes exist
4. `FullPdfReady`
5. `SourceSnapshot`
6. `BuildFinished { success: true }`

For a failed build:

1. `BuildStarted`
2. `Diagnostics`
3. `BuildFinished { success: false }`

The viewer should keep the last successful preview mounted across failed revisions.

## Viewport Prewarm Flow

The viewer emits `ViewportChanged` when the visible page set, zoom, or scroll position
changes:

```json
{
  "type": "viewport_changed",
  "zoom": 1.25,
  "current_page": 3,
  "scroll_top": 1440,
  "visible_pages": ["page-2", "page-3", "page-4"]
}
```

The daemon currently uses this message to prewarm likely page rasters and adjacent page
artifacts. It is a hint channel, not an acknowledgement protocol.

## HTTP Endpoints That Still Matter

WebSocket is not the only transport. The viewer still uses HTTP for demand-driven requests:

* `GET /api/state`
  Bootstraps the initial viewer snapshot, including `source_snapshot`.
* `GET /api/tiles/<rev>/<page_id>`
  Fetches tile manifests for the current viewport when tile mode is active.
* `GET /api/syncmap/<rev>/<page_id>`
  Fetches source/output mapping for a page.
* `GET /api/source-file/<rev>?file=...`
  Reads a single source file.
* `PUT /api/source-file`
  Saves an edited source file.
* `GET /api/source-jump/<rev>`
  Resolves source-to-preview jumps.
* `POST /api/open-source/<rev>`
  Resolves editor handoff and preview-only open-source flows.

That split is deliberate: websocket carries revision-wide push updates, while HTTP serves
idempotent point lookups and mutations.

## Reducer Contract

The viewer reducer is still meant to stay pure:

```ts
function reduce(state: ViewerState, msg: ServerMsg | UiMsg): ViewerState
```

WebSocket handlers, HTTP fetches, and DOM side effects stay outside the reducer. The core
tests in `web/packages/viewer-core/test/app.test.ts` lock the reducer and runtime contracts
around this message surface.
