import assert from "node:assert/strict";
import test from "node:test";

import {
  type BrowserBuildMetadata,
  type BrowserDrawOp,
  type BrowserFontAsset,
  type BrowserPagesArtifact,
  validateBrowserArtifacts
} from "../src/lib/browser-artifacts.ts";
import {
  browserDestinationId,
  browserGlyphPathData,
  browserLinkHref,
  browserPositionedGlyphOutlines,
  prepareDisplayListOps
} from "../src/lib/display-list-renderer.ts";

const textRun: Extract<BrowserDrawOp, { kind: "text_run" }> = {
  kind: "text_run",
  origin: { x: 10, y: 20 },
  text: "A ",
  font: {
    family: "serif",
    series: "regular",
    shape: "upright",
    size_pt: 10,
    role: "body"
  },
  size_pt: 10,
  approximate_advance_pt: 10,
  resolved_font: {
    face_id: "cmr10",
    postscript_name: "CMR10",
    glyph_id_kind: "type1_char_code",
    content_hash: "blake3:font"
  },
  glyphs: [
    { glyph_id: 65, advance_pt: 7, offset: { x: 0, y: 0 } },
    { glyph_id: 32, advance_pt: 3, offset: { x: 7, y: 0 } }
  ],
  clusters: [],
  source: {
    primary: { kind: "generated", stable_id: "run-a", description: "test" },
    related: [],
    expansion_stack: [],
    generated_by: "test",
    expansion_stack_truncated: false
  }
};

const fontAsset: BrowserFontAsset = {
  face_id: "cmr10",
  postscript_name: "CMR10",
  glyph_id_kind: "type1_char_code",
  content_hash: "blake3:font",
  glyphs: [
    {
      glyph_id: 32,
      commands: []
    },
    {
      glyph_id: 65,
      commands: [
        { kind: "move_to", x: 0, y: 0 },
        { kind: "curve_to", x1: 0.1, y1: 0.2, x2: 0.3, y2: 0.4, x: 0.5, y: 1 },
        { kind: "close" }
      ]
    }
  ]
};

function pagesArtifact(): BrowserPagesArtifact {
  return {
    schema_version: 2,
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
    assets: [],
    fonts: [{
      face_id: "cmr10",
      postscript_name: "CMR10",
      glyph_id_kind: "type1_char_code",
      content_hash: "blake3:font",
      glyphs: [{
        glyph_id: 65,
        commands: [
          { kind: "move_to", x: 0, y: 0 },
          { kind: "line_to", x: 0.5, y: 1 },
          { kind: "close" }
        ]
      }]
    }]
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

test("browser artifact validation rejects non-finite glyph outline coordinates", () => {
  const malformed = pagesArtifact();
  malformed.fonts[0]!.glyphs[0]!.commands = [
    { kind: "move_to", x: Number.NaN, y: 0 }
  ];

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
    /invalid browser font manifest/
  );
});

test("browser artifact validation rejects duplicate font and glyph identities", () => {
  const duplicateFont = pagesArtifact();
  duplicateFont.fonts.push(structuredClone(duplicateFont.fonts[0]!));
  assert.throws(
    () => validateBrowserArtifacts(duplicateFont, buildMetadata(), {
      revision: 4,
      page_count: 1,
      event_count: 12,
      diagnostic_count: 1
    }),
    /invalid browser font manifest/
  );

  const duplicateGlyph = pagesArtifact();
  duplicateGlyph.fonts[0]!.glyphs.push(structuredClone(duplicateGlyph.fonts[0]!.glyphs[0]!));
  assert.throws(
    () => validateBrowserArtifacts(duplicateGlyph, buildMetadata(), {
      revision: 4,
      page_count: 1,
      event_count: 12,
      diagnostic_count: 1
    }),
    /invalid browser font manifest/
  );
});

test("browser artifact validation rejects over-budget glyph command streams", () => {
  const oversized = pagesArtifact();
  oversized.fonts[0]!.glyphs[0]!.commands = Array.from(
    { length: 4_097 },
    () => ({ kind: "close" as const })
  );
  assert.throws(
    () => validateBrowserArtifacts(oversized, buildMetadata(), {
      revision: 4,
      page_count: 1,
      event_count: 12,
      diagnostic_count: 1
    }),
    /invalid browser font manifest/
  );
});

test("positioned Type1 glyphs use normalized paths and preserve empty glyphs", () => {
  assert.equal(
    browserGlyphPathData(fontAsset.glyphs[1]!),
    "M 0 0 C 0.1 0.2 0.3 0.4 0.5 1 Z"
  );
  assert.deepEqual(browserPositionedGlyphOutlines(textRun, [fontAsset]), [
    {
      glyph_id: 65,
      path: "M 0 0 C 0.1 0.2 0.3 0.4 0.5 1 Z",
      transform: "matrix(10 0 0 -10 10 20)"
    },
    {
      glyph_id: 32,
      path: "",
      transform: "matrix(10 0 0 -10 17 20)"
    }
  ]);
});

test("positioned glyph rendering falls back for identity or coverage mismatches", () => {
  assert.equal(browserPositionedGlyphOutlines(textRun, [{
    ...fontAsset,
    content_hash: "blake3:other"
  }]), null);
  assert.equal(browserPositionedGlyphOutlines(textRun, [{
    ...fontAsset,
    glyphs: fontAsset.glyphs.filter((glyph) => glyph.glyph_id !== 32)
  }]), null);
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
