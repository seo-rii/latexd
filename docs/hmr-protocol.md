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
    PreviewResyncRequired { preview_rev: RevId },
    PreviewCommitted {
        base_preview_rev: RevId,
        new_preview_rev: RevId,
        pdf_url: String,
        page_ids: Vec<String>,
        page_artifacts: Vec<PagePreviewArtifact>,
        ops: Vec<PagePatchOp>,
        source_snapshot: Vec<SourceSnapshotFile>,
        render_ir_artifacts: Option<RenderIrArtifactUrls>,
    },
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
    PreviewLease { rev: RevId },
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

### `PreviewCommitted`

The daemon publishes page order, immutable PDF/SVG URLs, source snapshot, and optional
render-IR URLs in a single message. `base_preview_rev` is the previously published preview;
`new_preview_rev` is the committed build. `ops` describes structural changes and helps
preserve the selected page. The complete `page_artifacts` list is authoritative.

The viewer accepts a new commit only when its applied revision equals `base_preview_rev`.
On a base mismatch it keeps the last-good preview and reloads `/api/state`; it does not
apply any subset of the operations. Duplicate or older commits are ignored. Page order and
source data change together, while prior sync maps are invalidated. A delayed valid commit
can update the preview even after a newer build has started, without rolling back that
newer build's diagnostic state.

The daemon advertises `preview_protocol_version: 1` in `/api/state`. Legacy `PatchPages`,
`FullPdfReady`, and `SourceSnapshot` variants remain decodable for older transports but
are no longer emitted by the daemon. After negotiating atomic previews the viewer ignores
these split messages. Reconnection always refreshes `/api/state`, even if no new build is
pending. A missed or stale state response is retried while a base-mismatch resync is pending.

Successful publication first prepares and syncs a manifest at
`.latexd/build/rev-<rev>/preview.pending`. After checking queued edits, an atomic rename to
`published-preview.json` is the commit point. Uncommitted drafts cannot replace the last
normal preview. Startup validates referenced artifacts and sources, restores the last
published preview, and reserves a revision beyond all existing build directories. A
missing/corrupt manifest or missing referenced artifact causes a clean preview rebuild;
old revision ids are still not reused. The published manifest is a GC root independently
of the recent-revision retention window.

### `BuildFinished`

Marks the end of the revision build. `success = false` means the previous successful preview
stays mounted while diagnostics update.

## Typical Successful Revision Flow

For a successful build, the daemon emits messages in this order:

1. `BuildStarted`
2. `Diagnostics`
3. `PreviewCommitted` with the complete page/source manifest
4. `BuildFinished { success: true }`

For a failed build:

1. `BuildStarted`
2. `Diagnostics`
3. `BuildFinished { success: false }`

The viewer should keep the last successful preview mounted across failed revisions.

## Full PDF Export

A native daemon preview publishes ordered page PDFs/SVGs, source/sync data, and the
renderer display list. Whole-document PDF and large diagnostic artifacts are omitted
from that path. The immutable full-PDF URL remains usable: requesting
`/artifacts/rev/<rev>/<document>.pdf` or `/artifacts/latest.pdf` exports the ordered pages
from `rev-<rev>/pdf-export.json` on demand. Existing external compiler PDFs are served
directly. CLI/oracle compilation keeps eager exports unless it opts into preview mode.

Concurrent downloads share one export job. Its revision pin outlives cancelled requests;
the resulting file is synced and renamed atomically, and failed exports never publish a
partial file. Export preserves each page's content, inherited geometry/resources, and
named destinations. Limits cover aggregate input bytes, decoded resources, object count,
and serialized output size. Startup can restore a published preview whose full PDF has
not yet been requested, provided its export recipe and referenced page artifacts are valid.

## Preview Ownership and Collection

Each WebSocket connection owns at most one preview revision. The viewer sends
`{ "type": "preview_lease", "rev": <applied revision> }` on socket open and after an
atomic preview commit or state reload. A build starting or failing does not move this
lease. Replacing the revision releases the old root; revision zero explicitly releases
it. Disconnect drops ownership automatically. An unknown, collected, or newer-than-published
revision is rejected with `PreviewResyncRequired`, and the client reloads `/api/state`.

Active client revisions and in-flight HTTP/render revisions join the recent manifests
and the durable published manifest as GC roots. The collector retains their referenced
artifact owner directories. New root acquisition is serialized with mark/sweep, while
RAII ownership release does not require a background cleanup task. A render queue envelope
owns its input revision through queueing and native execution even if the HTTP response
future is cancelled. Once clients and jobs release a revision, its unreferenced files can
be reclaimed on the next collection.

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
