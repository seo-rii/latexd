import assert from "node:assert/strict";
import test from "node:test";

import {
  type BrowserBuildMetadata,
  type BrowserPagesArtifact,
  validateBrowserArtifacts
} from "../src/lib/browser-artifacts.ts";
import {
  browserDestinationId,
  browserLinkHref,
  prepareDisplayListOps
} from "../src/lib/display-list-renderer.ts";

function pagesArtifact(): BrowserPagesArtifact {
  return {
    schema_version: 1,
    revision: 4,
    pages: [{
      page_id: "page-a",
      width_pt: 612,
      height_pt: 792,
      ops: [],
      source_spans: [],
      content_hash: "hash-a"
    }],
    changed_page_ids: ["page-a"],
    removed_page_ids: [],
    assets: []
  };
}

function buildMetadata(): BrowserBuildMetadata {
  return {
    schema_version: 1,
    revision: 4,
    compile_mode: "one_shot",
    event_count: 12,
    diagnostic_count: 1,
    pages: {
      total: 1,
      changed: 1,
      reused: 0,
      removed: 0
    }
  };
}

test("browser artifact validation accepts a coherent compiler-owned manifest", () => {
  assert.doesNotThrow(() => validateBrowserArtifacts(
    pagesArtifact(),
    buildMetadata(),
    {
      revision: 4,
      page_count: 1,
      event_count: 12,
      diagnostic_count: 1
    }
  ));
});

test("browser artifact validation rejects stale revisions", () => {
  assert.throws(
    () => validateBrowserArtifacts(
      pagesArtifact(),
      buildMetadata(),
      {
        revision: 5,
        page_count: 1,
        event_count: 12,
        diagnostic_count: 1
      }
    ),
    /wrong revision/
  );
});

test("browser artifact validation rejects malformed or duplicate asset entries", () => {
  const malformed = pagesArtifact();
  malformed.assets = [{ asset_ref: "", format: "svg" }];
  assert.throws(
    () => validateBrowserArtifacts(
      malformed,
      buildMetadata(),
      {
        revision: 4,
        page_count: 1,
        event_count: 12,
        diagnostic_count: 1
      }
    ),
    /invalid browser asset manifest/
  );

  const duplicate = pagesArtifact();
  duplicate.assets = [
    { asset_ref: "figure.svg", format: "svg", content_hash: "first" },
    { asset_ref: "figure.svg", format: "svg", content_hash: "second" }
  ];
  assert.throws(
    () => validateBrowserArtifacts(
      duplicate,
      buildMetadata(),
      {
        revision: 4,
        page_count: 1,
        event_count: 12,
        diagnostic_count: 1
      }
    ),
    /invalid browser asset manifest/
  );
});

test("display-list preparation applies nested clips until restore", () => {
  const prepared = prepareDisplayListOps([
    { kind: "save" },
    { kind: "clip_rect", x: 0, y: 0, width: 100, height: 100 },
    { kind: "save" },
    { kind: "clip_rect", x: 50, y: 40, width: 100, height: 80 },
    { kind: "rule", x: 60, y: 50, width: 10, height: 2 },
    { kind: "restore" },
    { kind: "rule", x: 20, y: 20, width: 10, height: 2 },
    { kind: "restore" },
    { kind: "rule", x: 5, y: 5, width: 10, height: 2 }
  ]);

  assert.deepEqual(prepared.ops.map((entry) => entry.clip_rect), [
    { x: 50, y: 40, width: 50, height: 60 },
    { x: 0, y: 0, width: 100, height: 100 },
    null
  ]);
  assert.deepEqual(prepared.diagnostics, []);
});

test("display-list preparation diagnoses unbalanced graphics state locally", () => {
  const prepared = prepareDisplayListOps([
    { kind: "restore" },
    { kind: "save" },
    { kind: "rule", x: 0, y: 0, width: 1, height: 1 }
  ]);

  assert.equal(prepared.ops.length, 1);
  assert.deepEqual(prepared.diagnostics, [
    "display list restored an empty graphics-state stack",
    "display list ended with 1 saved graphics state(s)"
  ]);
});

test("browser display-list links distinguish safe URLs and named destinations", () => {
  assert.equal(browserLinkHref("https://example.com/paper"), "https://example.com/paper");
  assert.equal(browserLinkHref("mailto:author@example.com"), "mailto:author@example.com");
  assert.equal(browserLinkHref("fig:inside"), "#destination-fig_3Ainside");
  assert.equal(browserLinkHref("#fig:inside"), "#destination-fig_3Ainside");
  assert.equal(browserDestinationId("fig:inside"), "destination-fig_3Ainside");
  assert.equal(browserLinkHref("javascript:alert(1)"), null);
  assert.equal(browserLinkHref("data:text/html,unsafe"), null);
});
