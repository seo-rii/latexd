# Renderer Session Plan

## Purpose

`latexd` already has `gs-cli`, `gs-api`, a runtime pool, raster caches, disk-backed cache, and viewport prewarm. The missing piece is a true long-lived renderer session that keeps Ghostscript page/device setup alive across cold misses.

This document narrows that work so it does not sprawl into unrelated M7/M8/M9 areas.

## Current State

- `CliRenderer` shells out per cold render.
- `GsApiRenderer` reuses `libgs` loading and runtime objects, but still pays `init_with_args` and device setup on a cold render.
- `latexd` caches rendered PNGs in memory and on disk, and deduplicates concurrent misses.
- Viewer-driven `viewport_changed` prewarms likely pages, but only after build output exists.
- `latexd` now also has a Phase 1 shared renderer actor/session wrapper at the daemon layer: full-page cold misses for the same document root + renderer mode go through one shared render lane instead of constructing a fresh renderer path per miss.
- Phase 2 has also started in a broader narrow form: the actor can now attach/detach recent revision page metadata, answer attached-page lookups before the HTTP handlers fall back to disk metadata, answer full revision page-metadata-set lookups for sync/source-jump style request paths, keep a small recent-revision window of attached revisions alive, expose that attached revision window through the debug surface, and retain/evict attached revisions through an actor-owned LRU + page-budget policy instead of build-loop `rev - N` detach.
- Phase 3 has also started in a narrow form: tile requests now go through the same actor/session lane and call renderer-native `render_tiles` instead of rendering a full PNG and cropping it inside the HTTP handler. The actor now also keeps a small recent rect-tile cache keyed by revision/page/content-hash/zoom/rect, so repeated identical tile requests and mixed cached+uncached batches can reuse cached tiles and only render missing rectangles. Broader rectangle batch ownership, per-page display-list retention, and `gs-cli` staying purely as fallback/oracle are still follow-up work.
- Phase 4 has also started in a narrow form: viewport prewarm no longer fills the full-page raster cache eagerly. It now prioritizes current page, then visible pages, then adjacent pages, suppresses duplicate in-flight warm work for the same revision/page/zoom bucket, and sends actor-owned prewarm requests that warm renderer-native tile/session state without writing PNG cache entries. The actor now also keeps a per-page warm-bucket budget so one page's zoom churn does not evict every other warmed page. Longer-lived warm ownership tuning is still follow-up work.
- Phase 5 has also started in a narrow form: the daemon now keeps actor spawn/restart/attach/detach/evict/full-render/tile-render/prewarm/fallback counters, a small recent event ring keyed by revision/page id, emits matching structured debug logs, and exposes them through a debug metrics surface. Render/tile/prewarm/fallback events now also carry coarse duration data in that recent-event/debug surface, and the debug snapshot also exposes aggregate latency summaries plus live attached-page-count / revision-window / page-budget context for render/tile/prewarm/fallback paths. Broader latency observability remains follow-up work.

## Constraints

- Ghostscript `gsapi_*` calls remain single-thread-affine.
- Transparent PDFs cannot rely on `display_update`; rectangle request must be the steady-state path.
- The renderer is a preview accelerator. It must not become the owner of document invalidation or checkpoint policy.

## Phase 1: Document Render Session

Target: one long-lived render actor per active revision family.

- Add `RenderSessionKey = (doc_root, compiler_profile, renderer_mode)`.
- Keep one actor/task per key, with a single-thread execution lane for all `gsapi_*` calls.
- Move current runtime-pool selection behind the actor so callers only ask for `render_page` / `render_tiles`.
- Teach the actor to load a page PDF once, keep the page/device context alive for a short idle window, and serve repeated zoom/tile requests without reinitializing Ghostscript immediately.

Done when:

- repeated cold page requests for the same revision hit the same actor/session
- `latexd` no longer constructs a fresh `GsApiRenderer` path for every miss

## Phase 2: Revision Attachment and Reuse

Target: separate renderer-session lifetime from revision artifact lifetime.

- Introduce explicit `attach_revision(rev, page_metadata)` and `detach_revision(rev)` messages.
- Let a session hold multiple nearby revisions so unchanged pages reused from `rev-N` can still be rasterized while `rev-(N+1)` is active.
- Track per-page source PDF path and content hash inside the actor, not only in HTTP handlers.
- Keep a small LRU of attached revisions; evict by idle time and memory budget.

Done when:

- mixed old/new page stacks no longer force ad hoc path lookups in request handlers
- reused page artifacts remain renderable after several incremental builds without rebuilding session state

## Phase 3: Rectangle-Request Tile Path

Target: stop rendering a full PNG just to crop tiles.

- Add a dedicated `GsApiTileRenderer` path that enters rectangle-request mode once per page/session.
- Keep a per-page display-list handle inside the actor.
- Convert viewport tile requests into rectangle batches and satisfy them directly from Ghostscript.
- Keep a small actor-owned recent rect cache so repeated identical tiles and mixed cached+uncached batches do not always re-render every rectangle.
- Keep `CliRenderer` as fallback and test oracle, but do not extend it further.

Done when:

- tile endpoints can render only requested rectangles
- repeated identical tile requests stay inside the actor-owned rect cache
- mixed cached+uncached tile batches only render missing rectangles
- full-page rasterization becomes fallback, not the primary tile path

## Phase 4: Session-Aware Prewarm

Target: align browser viewport hints with actor/session state.

- Change prewarm from “render this PNG now” to “ensure this page/session is attached and its display list is warm”.
- Prioritize current page, then visible pages, then adjacent pages.
- Carry zoom buckets into prewarm scheduling only after page/session warmup succeeds.

Done when:

- first visible tile after scroll usually reuses an attached page session
- prewarm does not create duplicate session work across concurrent viewers

## Phase 5: Failure and Metrics

Target: make the session path operable.

- Add counters for actor spawn, attach, detach, page warm, rectangle render, fallback full render, and crash/restart.
- Keep a small recent event ring keyed by revision/page id so restart/fallback churn is inspectable without tailing logs.
- If a session crashes, discard only renderer state and fall back to cached PNG or `gs-cli`; do not poison document state.
- Emit structured logs keyed by revision and page id.

Done when:

- renderer-session failures degrade to existing cache/fallback paths
- latency improvements and regressions are measurable

## Recommended Order

1. Phase 1 actor/session wrapper
2. Phase 2 revision attachment
3. Phase 5 metrics/failure hooks
4. Phase 3 rectangle-request tile rendering
5. Phase 4 session-aware prewarm tuning

This order keeps the first step small: stabilize ownership and lifecycle before chasing rectangle-request performance.
