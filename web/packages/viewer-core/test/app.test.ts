import test from "node:test";
import assert from "node:assert/strict";

import {
  formatSourceSelectionHash,
  initialState,
  normalizeSourceJumpRequest,
  parseSourceHashRequest,
  reduce,
  resolvedSourceRequestDetail,
  sourceRequestFromSelection,
  syncSelectionFromJumpContext,
  selectNearestSyncItem
} from "../src/index.ts";

test("successful build installs PDF url", () => {
  let state = reduce(initialState, {
    type: "build_started",
    rev: 2,
    changed_files: ["main.tex"]
  });
  state = reduce(state, {
    type: "full_pdf_ready",
    rev: 2,
    pdf_url: "/artifacts/rev/2/main.pdf",
    page_ids: ["page-0", "page-1"],
    page_artifacts: [
      {
        page_id: "page-0",
        pdf_url: "/artifacts/rev/2/pages/page-0.pdf",
        svg_url: "/artifacts/rev/2/pages/page-0.svg"
      },
      {
        page_id: "page-1",
        pdf_url: "/artifacts/rev/2/pages/page-1.pdf",
        svg_url: "/artifacts/rev/2/pages/page-1.svg"
      }
    ]
  });

  assert.equal(state.pdfUrl, "/artifacts/rev/2/main.pdf");
  assert.deepEqual(state.pageIds, ["page-0", "page-1"]);
  assert.deepEqual(state.pages, [
    {
      pageId: "page-0",
      pdfUrl: "/artifacts/rev/2/pages/page-0.pdf",
      svgUrl: "/artifacts/rev/2/pages/page-0.svg"
    },
    {
      pageId: "page-1",
      pdfUrl: "/artifacts/rev/2/pages/page-1.pdf",
      svgUrl: "/artifacts/rev/2/pages/page-1.svg"
    }
  ]);
  assert.equal(state.lastBuildSucceeded, true);
});

test("successful build clears stale render IR artifact links until snapshot refresh", () => {
  const state = reduce({
    ...initialState,
    renderIrArtifacts: {
      events_url: "/artifacts/rev/1/render-ir/events.json"
    }
  }, {
    type: "full_pdf_ready",
    rev: 2,
    pdf_url: "/artifacts/rev/2/main.pdf",
    page_ids: ["page-0"],
    page_artifacts: [{
      page_id: "page-0",
      pdf_url: "/artifacts/rev/2/pages/page-0.pdf",
      svg_url: "/artifacts/rev/2/pages/page-0.svg"
    }]
  });

  assert.equal(state.renderIrArtifacts, null);
});

test("failed build preserves last good preview", () => {
  let state = reduce(initialState, {
    type: "full_pdf_ready",
    rev: 1,
    pdf_url: "/artifacts/rev/1/main.pdf",
    page_ids: ["page-0"],
    page_artifacts: [{
      page_id: "page-0",
      pdf_url: "/artifacts/rev/1/pages/page-0.pdf",
      svg_url: "/artifacts/rev/1/pages/page-0.svg"
    }]
  });
  state = reduce(state, {
    type: "build_started",
    rev: 2,
    changed_files: ["main.tex"]
  });
  state = reduce(state, {
    type: "build_finished",
    rev: 2,
    success: false
  });

  assert.equal(state.pdfUrl, "/artifacts/rev/1/main.pdf");
  assert.equal(state.lastBuildSucceeded, false);
});

test("stale revisions are ignored", () => {
  const state = reduce(
    reduce(initialState, {
      type: "full_pdf_ready",
      rev: 5,
      pdf_url: "/artifacts/rev/5/main.pdf",
      page_ids: ["page-0", "page-1"],
      page_artifacts: [
        {
          page_id: "page-0",
          pdf_url: "/artifacts/rev/5/pages/page-0.pdf",
          svg_url: "/artifacts/rev/5/pages/page-0.svg"
        },
        {
          page_id: "page-1",
          pdf_url: "/artifacts/rev/5/pages/page-1.pdf",
          svg_url: "/artifacts/rev/5/pages/page-1.svg"
        }
      ]
    }),
    {
      type: "full_pdf_ready",
      rev: 4,
      pdf_url: "/artifacts/rev/4/main.pdf",
      page_ids: ["old"],
      page_artifacts: [{
        page_id: "old",
        pdf_url: "/artifacts/rev/4/pages/old.pdf",
        svg_url: "/artifacts/rev/4/pages/old.svg"
      }]
    }
  );

  assert.equal(state.pdfUrl, "/artifacts/rev/5/main.pdf");
});

test("page and zoom survive preview refresh", () => {
  let state = reduce(initialState, { type: "ui_page_changed", page: 7 });
  state = reduce(state, { type: "ui_zoom_changed", zoom: 1.8 });
  state = reduce(state, {
    type: "full_pdf_ready",
    rev: 3,
    pdf_url: "/artifacts/rev/3/main.pdf",
    page_ids: [
      "page-0",
      "page-1",
      "page-2",
      "page-3",
      "page-4",
      "page-5",
      "page-6"
    ],
    page_artifacts: [
      { page_id: "page-0", pdf_url: "/artifacts/rev/3/pages/page-0.pdf", svg_url: "/artifacts/rev/3/pages/page-0.svg" },
      { page_id: "page-1", pdf_url: "/artifacts/rev/3/pages/page-1.pdf", svg_url: "/artifacts/rev/3/pages/page-1.svg" },
      { page_id: "page-2", pdf_url: "/artifacts/rev/3/pages/page-2.pdf", svg_url: "/artifacts/rev/3/pages/page-2.svg" },
      { page_id: "page-3", pdf_url: "/artifacts/rev/3/pages/page-3.pdf", svg_url: "/artifacts/rev/3/pages/page-3.svg" },
      { page_id: "page-4", pdf_url: "/artifacts/rev/3/pages/page-4.pdf", svg_url: "/artifacts/rev/3/pages/page-4.svg" },
      { page_id: "page-5", pdf_url: "/artifacts/rev/3/pages/page-5.pdf", svg_url: "/artifacts/rev/3/pages/page-5.svg" },
      { page_id: "page-6", pdf_url: "/artifacts/rev/3/pages/page-6.pdf", svg_url: "/artifacts/rev/3/pages/page-6.svg" }
    ]
  });

  assert.equal(state.currentPage, 7);
  assert.equal(state.zoom, 1.8);
});

test("patch page messages preserve unchanged page pdf sources", () => {
  const state = reduce(
    reduce(initialState, {
      type: "full_pdf_ready",
      rev: 1,
      pdf_url: "/artifacts/rev/1/main.pdf",
      page_ids: ["page-0", "page-1", "page-2"],
      page_artifacts: [
        { page_id: "page-0", pdf_url: "/artifacts/rev/1/pages/page-0.pdf", svg_url: "/artifacts/rev/1/pages/page-0.svg" },
        { page_id: "page-1", pdf_url: "/artifacts/rev/1/pages/page-1.pdf", svg_url: "/artifacts/rev/1/pages/page-1.svg" },
        { page_id: "page-2", pdf_url: "/artifacts/rev/1/pages/page-2.pdf", svg_url: "/artifacts/rev/1/pages/page-2.svg" }
      ]
    }),
    {
      type: "patch_pages",
      rev: 2,
      ops: [
        {
          op: "insert_page",
          index: 1,
          page_id: "page-inserted",
          pdf_url: "/artifacts/rev/2/pages/page-inserted.pdf",
          svg_url: "/artifacts/rev/2/pages/page-inserted.svg"
        },
      ]
    }
  );
  assert.deepEqual(state.pages, [
    { pageId: "page-0", pdfUrl: "/artifacts/rev/1/pages/page-0.pdf", svgUrl: "/artifacts/rev/1/pages/page-0.svg" },
    { pageId: "page-inserted", pdfUrl: "/artifacts/rev/2/pages/page-inserted.pdf", svgUrl: "/artifacts/rev/2/pages/page-inserted.svg" },
    { pageId: "page-1", pdfUrl: "/artifacts/rev/1/pages/page-1.pdf", svgUrl: "/artifacts/rev/1/pages/page-1.svg" },
    { pageId: "page-2", pdfUrl: "/artifacts/rev/1/pages/page-2.pdf", svgUrl: "/artifacts/rev/1/pages/page-2.svg" }
  ]);
  const refreshed = reduce(state, {
    type: "full_pdf_ready",
    rev: 2,
    pdf_url: "/artifacts/rev/2/main.pdf",
    page_ids: ["page-0", "page-inserted", "page-1", "page-2"],
    page_artifacts: [
      { page_id: "page-0", pdf_url: "/artifacts/rev/1/pages/page-0.pdf", svg_url: "/artifacts/rev/1/pages/page-0.svg" },
      { page_id: "page-inserted", pdf_url: "/artifacts/rev/2/pages/page-inserted.pdf", svg_url: "/artifacts/rev/2/pages/page-inserted.svg" },
      { page_id: "page-1", pdf_url: "/artifacts/rev/1/pages/page-1.pdf", svg_url: "/artifacts/rev/1/pages/page-1.svg" },
      { page_id: "page-2", pdf_url: "/artifacts/rev/1/pages/page-2.pdf", svg_url: "/artifacts/rev/1/pages/page-2.svg" }
    ]
  });

  assert.deepEqual(refreshed.pageIds, ["page-0", "page-inserted", "page-1", "page-2"]);
  assert.deepEqual(refreshed.pages, [
    { pageId: "page-0", pdfUrl: "/artifacts/rev/1/pages/page-0.pdf", svgUrl: "/artifacts/rev/1/pages/page-0.svg" },
    { pageId: "page-inserted", pdfUrl: "/artifacts/rev/2/pages/page-inserted.pdf", svgUrl: "/artifacts/rev/2/pages/page-inserted.svg" },
    { pageId: "page-1", pdfUrl: "/artifacts/rev/1/pages/page-1.pdf", svgUrl: "/artifacts/rev/1/pages/page-1.svg" },
    { pageId: "page-2", pdfUrl: "/artifacts/rev/1/pages/page-2.pdf", svgUrl: "/artifacts/rev/1/pages/page-2.svg" }
  ]);
  assert.equal(refreshed.pdfUrl, "/artifacts/rev/2/main.pdf");
});

test("page selection is clamped to known page inventory", () => {
  let state = reduce(initialState, {
    type: "full_pdf_ready",
    rev: 4,
    pdf_url: "/artifacts/rev/4/main.pdf",
    page_ids: ["page-0", "page-1"],
    page_artifacts: [
      { page_id: "page-0", pdf_url: "/artifacts/rev/4/pages/page-0.pdf", svg_url: "/artifacts/rev/4/pages/page-0.svg" },
      { page_id: "page-1", pdf_url: "/artifacts/rev/4/pages/page-1.pdf", svg_url: "/artifacts/rev/4/pages/page-1.svg" }
    ]
  });
  state = reduce(state, { type: "ui_page_changed", page: 9 });

  assert.equal(state.currentPage, 2);
});

test("patch replace updates only the changed page source", () => {
  let state = reduce(initialState, {
    type: "full_pdf_ready",
    rev: 4,
    pdf_url: "/artifacts/rev/4/main.pdf",
    page_ids: ["page-a", "page-b"],
    page_artifacts: [
      { page_id: "page-a", pdf_url: "/artifacts/rev/4/pages/page-a.pdf", svg_url: "/artifacts/rev/4/pages/page-a.svg" },
      { page_id: "page-b", pdf_url: "/artifacts/rev/4/pages/page-b.pdf", svg_url: "/artifacts/rev/4/pages/page-b.svg" }
    ]
  });
  state = reduce(state, {
    type: "patch_pages",
    rev: 5,
    ops: [{
      op: "replace_page",
      index: 1,
      page_id: "page-b2",
      pdf_url: "/artifacts/rev/5/pages/page-b2.pdf",
      svg_url: "/artifacts/rev/5/pages/page-b2.svg"
    }]
  });
  assert.deepEqual(state.pages, [
    { pageId: "page-a", pdfUrl: "/artifacts/rev/4/pages/page-a.pdf", svgUrl: "/artifacts/rev/4/pages/page-a.svg" },
    { pageId: "page-b2", pdfUrl: "/artifacts/rev/5/pages/page-b2.pdf", svgUrl: "/artifacts/rev/5/pages/page-b2.svg" }
  ]);
  state = reduce(state, {
    type: "full_pdf_ready",
    rev: 5,
    pdf_url: "/artifacts/rev/5/main.pdf",
    page_ids: ["page-a", "page-b2"],
    page_artifacts: [
      { page_id: "page-a", pdf_url: "/artifacts/rev/4/pages/page-a.pdf", svg_url: "/artifacts/rev/4/pages/page-a.svg" },
      { page_id: "page-b2", pdf_url: "/artifacts/rev/5/pages/page-b2.pdf", svg_url: "/artifacts/rev/5/pages/page-b2.svg" }
    ]
  });

  assert.deepEqual(state.pages, [
    { pageId: "page-a", pdfUrl: "/artifacts/rev/4/pages/page-a.pdf", svgUrl: "/artifacts/rev/4/pages/page-a.svg" },
    { pageId: "page-b2", pdfUrl: "/artifacts/rev/5/pages/page-b2.pdf", svgUrl: "/artifacts/rev/5/pages/page-b2.svg" }
  ]);
});

test("syncmap payloads install only for the applied revision", () => {
  let state = reduce(initialState, {
    type: "full_pdf_ready",
    rev: 8,
    pdf_url: "/artifacts/rev/8/main.pdf",
    page_ids: ["page-a"],
    page_artifacts: [
      { page_id: "page-a", pdf_url: "/artifacts/rev/8/pages/page-a.pdf", svg_url: "/artifacts/rev/8/pages/page-a.svg" }
    ]
  });
  state = reduce(state, {
    type: "ui_syncmap_ready",
    rev: 7,
    page_id: "page-a",
    page_width_px: 612,
    page_height_px: 792,
    page_source_start_utf8: 0,
    page_source_end_utf8: 10,
    page_output_start_utf8: 0,
    page_output_end_utf8: 24,
    items: [{ file: "main.tex", start_utf8: 0, end_utf8: 10, start_line: 1, end_line: 2, left_px: 72, right_px: 144, top_px: 0, bottom_px: 100 }]
  });
  assert.deepEqual(state.syncMaps, {});

  state = reduce(state, {
    type: "ui_syncmap_ready",
    rev: 8,
    page_id: "page-a",
    page_width_px: 612,
    page_height_px: 792,
    page_source_start_utf8: 0,
    page_source_end_utf8: 10,
    page_output_start_utf8: 0,
    page_output_end_utf8: 24,
    items: [{ file: "main.tex", start_utf8: 0, end_utf8: 10, start_line: 1, end_line: 2, left_px: 72, right_px: 144, top_px: 0, bottom_px: 100 }]
  });
  assert.deepEqual(state.syncMaps, {
    "page-a": {
      rev: 8,
      page_width_px: 612,
      page_height_px: 792,
      page_source_start_utf8: 0,
      page_source_end_utf8: 10,
      page_output_start_utf8: 0,
      page_output_end_utf8: 24,
      items: [{ file: "main.tex", start_utf8: 0, end_utf8: 10, start_line: 1, end_line: 2, left_px: 72, right_px: 144, top_px: 0, bottom_px: 100 }]
    }
  });
});

test("syncmap refresh clears stale selection outside the new page source window", () => {
  let state = reduce(initialState, {
    type: "full_pdf_ready",
    rev: 13,
    pdf_url: "/artifacts/rev/13/main.pdf",
    page_ids: ["page-a"],
    page_artifacts: [
      { page_id: "page-a", pdf_url: "/artifacts/rev/13/pages/page-a.pdf", svg_url: "/artifacts/rev/13/pages/page-a.svg" }
    ]
  });
  state = reduce(state, {
    type: "ui_sync_selected",
    item: {
      pageId: "page-a",
      pageHeightPx: 792,
      file: "main.tex",
      startUtf8: 30,
      endUtf8: 40,
      outputStartUtf8: 40,
      outputEndUtf8: 56,
      startLine: 4,
      endLine: 4,
      topPx: 220,
      bottomPx: 280
    }
  });
  state = reduce(state, {
    type: "ui_source_hover_resolved",
    page_id: "page-a",
    page_index: 0,
    item: {
      pageId: "page-a",
      pageHeightPx: 792,
      file: "main.tex",
      startUtf8: 18,
      endUtf8: 24,
      outputStartUtf8: 24,
      outputEndUtf8: 32,
      startLine: 3,
      endLine: 3,
      topPx: 140,
      bottomPx: 180
    }
  });
  state = reduce(state, {
    type: "ui_syncmap_ready",
    rev: 13,
    page_id: "page-a",
    page_width_px: 612,
    page_height_px: 792,
    page_source_start_utf8: 0,
    page_source_end_utf8: 10,
    page_output_start_utf8: 0,
    page_output_end_utf8: 24,
    items: [{ file: "main.tex", start_utf8: 0, end_utf8: 10, start_line: 1, end_line: 2, left_px: 72, right_px: 144, top_px: 0, bottom_px: 100 }]
  });

  assert.equal(state.selectedSource, null);
  assert.equal(state.hoveredSource, null);
});

test("syncmap refresh clears stale selection outside the new page output window", () => {
  let state = reduce(initialState, {
    type: "full_pdf_ready",
    rev: 13,
    pdf_url: "/artifacts/rev/13/main.pdf",
    page_ids: ["page-a"],
    page_artifacts: [
      { page_id: "page-a", pdf_url: "/artifacts/rev/13/pages/page-a.pdf", svg_url: "/artifacts/rev/13/pages/page-a.svg" }
    ]
  });
  state = reduce(state, {
    type: "ui_sync_selected",
    item: {
      pageId: "page-a",
      pageHeightPx: 792,
      file: "main.tex",
      startUtf8: 4,
      endUtf8: 10,
      outputStartUtf8: 40,
      outputEndUtf8: 56,
      startLine: 2,
      endLine: 2,
      topPx: 220,
      bottomPx: 280
    }
  });
  state = reduce(state, {
    type: "ui_syncmap_ready",
    rev: 13,
    page_id: "page-a",
    page_width_px: 612,
    page_height_px: 792,
    page_source_start_utf8: 0,
    page_source_end_utf8: 10,
    page_output_start_utf8: 0,
    page_output_end_utf8: 24,
    items: [{
      file: "main.tex",
      start_utf8: 0,
      end_utf8: 10,
      output_start_utf8: 0,
      output_end_utf8: 24,
      start_line: 1,
      end_line: 2,
      left_px: 72,
      right_px: 144,
      top_px: 0,
      bottom_px: 100
    }]
  });

  assert.equal(state.selectedSource, null);
});

test("syncmap refresh reanchors matching selection by stable item id", () => {
  let state = reduce(initialState, {
    type: "full_pdf_ready",
    rev: 13,
    pdf_url: "/artifacts/rev/13/main.pdf",
    page_ids: ["page-a"],
    page_artifacts: [
      { page_id: "page-a", pdf_url: "/artifacts/rev/13/pages/page-a.pdf", svg_url: "/artifacts/rev/13/pages/page-a.svg" }
    ]
  });
  state = reduce(state, {
    type: "ui_sync_selected",
    item: {
      itemId: "page-a:main.tex:0:10:1:2",
      pageId: "page-a",
      pageWidthPx: 612,
      pageHeightPx: 792,
      file: "main.tex",
      startUtf8: 0,
      endUtf8: 10,
      outputStartUtf8: 0,
      outputEndUtf8: 24,
      pageSourceStartUtf8: 0,
      pageSourceEndUtf8: 10,
      pageOutputStartUtf8: 0,
      pageOutputEndUtf8: 24,
      startLine: 1,
      endLine: 2,
      leftPx: 72,
      rightPx: 144,
      topPx: 0,
      bottomPx: 100,
      sourceHash: "#src=main.tex&line=1"
    }
  });
  state = reduce(state, {
    type: "ui_syncmap_ready",
    rev: 13,
    page_id: "page-a",
    page_width_px: 612,
    page_height_px: 792,
    page_source_start_utf8: 0,
    page_source_end_utf8: 12,
    page_output_start_utf8: 100,
    page_output_end_utf8: 140,
    items: [{
      item_id: "page-a:main.tex:0:10:1:2",
      file: "main.tex",
      start_utf8: 0,
      end_utf8: 10,
      output_start_utf8: 100,
      output_end_utf8: 120,
      start_line: 1,
      end_line: 2,
      left_px: 80,
      right_px: 188,
      top_px: 120,
      bottom_px: 180
    }]
  });

  assert.deepEqual(state.selectedSource, {
    itemId: "page-a:main.tex:0:10:1:2",
    pageId: "page-a",
    pageWidthPx: 612,
    pageHeightPx: 792,
    file: "main.tex",
    startUtf8: 0,
    endUtf8: 10,
    outputStartUtf8: 100,
    outputEndUtf8: 120,
    pageSourceStartUtf8: 0,
    pageSourceEndUtf8: 12,
    pageOutputStartUtf8: 100,
    pageOutputEndUtf8: 140,
    startLine: 1,
    endLine: 2,
    leftPx: 80,
    rightPx: 188,
    topPx: 120,
    bottomPx: 180,
    sourceHash: "#src=main.tex&line=1"
  });
});

test("source file payloads install only for the applied revision", () => {
  let state = reduce(initialState, {
    type: "full_pdf_ready",
    rev: 8,
    pdf_url: "/artifacts/rev/8/main.pdf",
    page_ids: ["page-a"],
    page_artifacts: [
      { page_id: "page-a", pdf_url: "/artifacts/rev/8/pages/page-a.pdf", svg_url: "/artifacts/rev/8/pages/page-a.svg" }
    ]
  });
  state = reduce(state, {
    type: "ui_source_file_ready",
    rev: 7,
    file: "main.tex",
    content: "old\n",
    line_count: 1
  });
  assert.deepEqual(state.sourceFiles, {});

  state = reduce(state, {
    type: "ui_source_file_ready",
    rev: 8,
    file: "main.tex",
    content: "lead\nbody\n",
    line_count: 2
  });
  assert.deepEqual(state.sourceFiles, {
    "main.tex": {
      rev: 8,
      content: "lead\nbody\n",
      lineCount: 2
    }
  });
});

test("source snapshot payloads replace source files only for the applied revision", () => {
  let state = reduce(initialState, {
    type: "full_pdf_ready",
    rev: 8,
    pdf_url: "/artifacts/rev/8/main.pdf",
    page_ids: ["page-a"],
    page_artifacts: [
      { page_id: "page-a", pdf_url: "/artifacts/rev/8/pages/page-a.pdf", svg_url: "/artifacts/rev/8/pages/page-a.svg" }
    ]
  });
  state = reduce(state, {
    type: "source_snapshot",
    rev: 7,
    files: [
      { file: "main.tex", content: "old\n", line_count: 1 }
    ]
  });
  assert.deepEqual(state.sourceFiles, {});

  state = reduce(state, {
    type: "source_snapshot",
    rev: 9,
    files: [
      { file: "future.tex", content: "future\n", line_count: 1 }
    ]
  });
  assert.deepEqual(state.sourceFiles, {});

  state = reduce(state, {
    type: "source_snapshot",
    rev: 8,
    files: [
      { file: "main.tex", content: "lead\nbody\n", line_count: 2 },
      { file: "sections/intro.tex", content: "nested\n", line_count: 1 }
    ]
  });
  assert.deepEqual(state.sourceFiles, {
    "main.tex": {
      rev: 8,
      content: "lead\nbody\n",
      lineCount: 2
    },
    "sections/intro.tex": {
      rev: 8,
      content: "nested\n",
      lineCount: 1
    }
  });
});

test("nearest sync item picks the closest preview band", () => {
  const syncMap = {
    page_width_px: 612,
    page_height_px: 300,
    items: [
      { file: "main.tex", start_utf8: 0, end_utf8: 4, left_px: 72, right_px: 210, top_px: 0, bottom_px: 60 },
      { file: "body.tex", start_utf8: 4, end_utf8: 12, left_px: 72, right_px: 280, top_px: 60, bottom_px: 180 },
      { file: "tail.tex", start_utf8: 12, end_utf8: 20, left_px: 72, right_px: 180, top_px: 180, bottom_px: 300 }
    ]
  };

  assert.equal(selectNearestSyncItem(syncMap, 20)?.file, "main.tex");
  assert.equal(selectNearestSyncItem(syncMap, 170)?.file, "body.tex");
  assert.equal(selectNearestSyncItem(syncMap, 295)?.file, "tail.tex");
});

test("nearest sync item uses x and y distance when box geometry is available", () => {
  const syncMap = {
    page_width_px: 612,
    page_height_px: 200,
    items: [
      { file: "left.tex", start_utf8: 0, end_utf8: 4, left_px: 72, right_px: 180, top_px: 40, bottom_px: 90 },
      { file: "right.tex", start_utf8: 5, end_utf8: 9, left_px: 260, right_px: 360, top_px: 40, bottom_px: 90 }
    ]
  };

  assert.equal(selectNearestSyncItem(syncMap, 60, 100)?.file, "left.tex");
  assert.equal(selectNearestSyncItem(syncMap, 60, 320)?.file, "right.tex");
});

test("source jump request normalization accepts offset and line forms", () => {
  assert.deepEqual(normalizeSourceJumpRequest("main.tex", 12.7), {
    file: "main.tex",
    offset: 13,
    line: null
  });
  assert.deepEqual(normalizeSourceJumpRequest({ file: "main.tex", offset: 12.7 }), {
    file: "main.tex",
    offset: 13,
    line: null
  });
  assert.deepEqual(normalizeSourceJumpRequest("main.tex", -0.4), {
    file: "main.tex",
    offset: 0,
    line: null
  });
  assert.deepEqual(normalizeSourceJumpRequest({ file: "main.tex", offset: -0.4 }), {
    file: "main.tex",
    offset: 0,
    line: null
  });
  assert.deepEqual(normalizeSourceJumpRequest("#src=main.tex&line=2&column=4"), {
    file: "main.tex",
    offset: null,
    line: 2,
    column: 4,
    source_hash: "#src=main.tex&line=2&column=4"
  });
  assert.deepEqual(normalizeSourceJumpRequest({ file: "main.tex", line: 3.2 }), {
    file: "main.tex",
    offset: null,
    line: 3
  });
  assert.deepEqual(normalizeSourceJumpRequest("main.tex", { line: 3.2, column: 4.7 }), {
    file: "main.tex",
    offset: null,
    line: 3,
    column: 5
  });
  assert.deepEqual(normalizeSourceJumpRequest({ file: "main.tex", line: 3.2, column: 4.7 }), {
    file: "main.tex",
    offset: null,
    line: 3,
    column: 5
  });
  assert.deepEqual(normalizeSourceJumpRequest("main.tex", { line: 0, column: 0 }), {
    file: "main.tex",
    offset: null,
    line: 1,
    column: 1
  });
  assert.deepEqual(normalizeSourceJumpRequest({ file: "main.tex", line: 0, column: 0 }), {
    file: "main.tex",
    offset: null,
    line: 1,
    column: 1
  });
  assert.deepEqual(
    normalizeSourceJumpRequest({ sourceHash: "#src=sections%2Fintro.tex&line=7&column=3" }),
    {
      file: "sections/intro.tex",
      offset: null,
      line: 7,
      column: 3,
      source_hash: "#src=sections%2Fintro.tex&line=7&column=3"
    }
  );
  assert.deepEqual(
    normalizeSourceJumpRequest({ source_hash: "#src=main.tex&offset=12" }),
    {
      file: "main.tex",
      offset: 12,
      line: null,
      source_hash: "#src=main.tex&offset=12"
    }
  );
  const offsetRequest = normalizeSourceJumpRequest({ sourceHash: "#src=main.tex&offset=12" });
  assert.deepEqual(offsetRequest, {
    file: "main.tex",
    offset: 12,
    line: null,
    source_hash: "#src=main.tex&offset=12"
  });
  assert.deepEqual(
    normalizeSourceJumpRequest({ sourceHash: "src=main.tex&offset=12" }),
    offsetRequest
  );
  assert.deepEqual(
    normalizeSourceJumpRequest({ source_hash: "src=main.tex&offset=12" }),
    offsetRequest
  );
  assert.deepEqual(
    normalizeSourceJumpRequest("src=main.tex&offset=12"),
    offsetRequest
  );
  const encodedOffsetRequest = normalizeSourceJumpRequest({ sourceHash: "#src=sections%2Fintro.tex&offset=12" });
  assert.deepEqual(encodedOffsetRequest, {
    file: "sections/intro.tex",
    offset: 12,
    line: null,
    source_hash: "#src=sections%2Fintro.tex&offset=12"
  });
  assert.deepEqual(
    normalizeSourceJumpRequest({ sourceHash: "src=sections%2Fintro.tex&offset=12" }),
    encodedOffsetRequest
  );
  assert.deepEqual(
    normalizeSourceJumpRequest({ source_hash: "src=sections%2Fintro.tex&offset=12" }),
    encodedOffsetRequest
  );
  assert.deepEqual(
    normalizeSourceJumpRequest("src=sections%2Fintro.tex&offset=12"),
    encodedOffsetRequest
  );
  const zeroOffsetRequest = normalizeSourceJumpRequest({ sourceHash: "#src=main.tex&offset=0" });
  assert.deepEqual(zeroOffsetRequest, {
    file: "main.tex",
    offset: 0,
    line: null,
    source_hash: "#src=main.tex&offset=0"
  });
  assert.deepEqual(
    normalizeSourceJumpRequest({ source_hash: "#src=main.tex&offset=0" }),
    zeroOffsetRequest
  );
  assert.deepEqual(
    normalizeSourceJumpRequest({ sourceHash: "src=main.tex&offset=0" }),
    zeroOffsetRequest
  );
  assert.deepEqual(
    normalizeSourceJumpRequest({ source_hash: "src=main.tex&offset=0" }),
    zeroOffsetRequest
  );
  assert.deepEqual(
    normalizeSourceJumpRequest("src=main.tex&offset=0"),
    zeroOffsetRequest
  );
  assert.equal(
    formatSourceSelectionHash({
      file: zeroOffsetRequest.file,
      startUtf8: zeroOffsetRequest.offset
    }),
    "#src=main.tex&offset=0"
  );
  const encodedZeroOffsetRequest = normalizeSourceJumpRequest({ sourceHash: "#src=sections%2Fintro.tex&offset=0" });
  assert.deepEqual(encodedZeroOffsetRequest, {
    file: "sections/intro.tex",
    offset: 0,
    line: null,
    source_hash: "#src=sections%2Fintro.tex&offset=0"
  });
  assert.deepEqual(
    normalizeSourceJumpRequest({ sourceHash: "src=sections%2Fintro.tex&offset=0" }),
    encodedZeroOffsetRequest
  );
  assert.deepEqual(
    normalizeSourceJumpRequest({ source_hash: "src=sections%2Fintro.tex&offset=0" }),
    encodedZeroOffsetRequest
  );
  assert.deepEqual(
    normalizeSourceJumpRequest("src=sections%2Fintro.tex&offset=0"),
    encodedZeroOffsetRequest
  );
  assert.equal(
    formatSourceSelectionHash({
      file: encodedZeroOffsetRequest.file,
      startUtf8: encodedZeroOffsetRequest.offset
    }),
    "#src=sections%2Fintro.tex&offset=0"
  );
  const explicitColumnOneRequest = normalizeSourceJumpRequest({ sourceHash: "#src=main.tex&line=7&column=1" });
  assert.deepEqual(explicitColumnOneRequest, {
    file: "main.tex",
    offset: null,
    line: 7,
    column: 1,
    source_hash: "#src=main.tex&line=7&column=1"
  });
  assert.deepEqual(
    normalizeSourceJumpRequest({ source_hash: "#src=main.tex&line=7&column=1" }),
    explicitColumnOneRequest
  );
  assert.deepEqual(
    normalizeSourceJumpRequest({ sourceHash: "src=main.tex&line=7&column=1" }),
    explicitColumnOneRequest
  );
  assert.deepEqual(
    normalizeSourceJumpRequest({ source_hash: "src=main.tex&line=7&column=1" }),
    explicitColumnOneRequest
  );
  assert.deepEqual(
    normalizeSourceJumpRequest("src=main.tex&line=7&column=1"),
    explicitColumnOneRequest
  );
  assert.equal(
    formatSourceSelectionHash({
      file: explicitColumnOneRequest.file,
      startLine: explicitColumnOneRequest.line,
      startColumn: explicitColumnOneRequest.column
    }),
    "#src=main.tex&line=7"
  );
  const encodedExplicitColumnOneRequest = normalizeSourceJumpRequest({
    sourceHash: "#src=sections%2Fintro.tex&line=7&column=1"
  });
  assert.deepEqual(encodedExplicitColumnOneRequest, {
    file: "sections/intro.tex",
    offset: null,
    line: 7,
    column: 1,
    source_hash: "#src=sections%2Fintro.tex&line=7&column=1"
  });
  assert.deepEqual(
    normalizeSourceJumpRequest({ sourceHash: "src=sections%2Fintro.tex&line=7&column=1" }),
    encodedExplicitColumnOneRequest
  );
  assert.deepEqual(
    normalizeSourceJumpRequest({ source_hash: "src=sections%2Fintro.tex&line=7&column=1" }),
    encodedExplicitColumnOneRequest
  );
  assert.deepEqual(
    normalizeSourceJumpRequest("src=sections%2Fintro.tex&line=7&column=1"),
    encodedExplicitColumnOneRequest
  );
  assert.equal(
    formatSourceSelectionHash({
      file: encodedExplicitColumnOneRequest.file,
      startLine: encodedExplicitColumnOneRequest.line,
      startColumn: encodedExplicitColumnOneRequest.column
    }),
    "#src=sections%2Fintro.tex&line=7"
  );
  assert.equal(normalizeSourceJumpRequest({ file: "main.tex" }), null);
});

test("source selection hash formatting and parsing roundtrip line-based links", () => {
  const hash = formatSourceSelectionHash({
    file: "sections/intro.tex",
    startLine: 7,
    startColumn: 3,
    startUtf8: 42
  });

  assert.equal(hash, "#src=sections%2Fintro.tex&line=7&column=3");
  assert.deepEqual(parseSourceHashRequest(hash), {
    file: "sections/intro.tex",
    offset: null,
    line: 7,
    column: 3
  });
  assert.deepEqual(parseSourceHashRequest("#src=sections%2Fintro.tex&line=7"), {
    file: "sections/intro.tex",
    offset: null,
    line: 7
  });
});

test("source selection hash canonicalizes explicit column one to the omitted form", () => {
  const canonicalHash = formatSourceSelectionHash({
    file: "main.tex",
    startLine: 7,
    startColumn: 1,
    startUtf8: 42
  });
  const explicitColumnOneRequest = parseSourceHashRequest("#src=main.tex&line=7&column=1");

  assert.equal(canonicalHash, "#src=main.tex&line=7");
  assert.deepEqual(explicitColumnOneRequest, {
    file: "main.tex",
    offset: null,
    line: 7,
    column: 1
  });
  assert.equal(
    formatSourceSelectionHash({
      file: explicitColumnOneRequest.file,
      startLine: explicitColumnOneRequest.line,
      startColumn: explicitColumnOneRequest.column
    }),
    canonicalHash
  );
});

test("source selection hash falls back to utf8 offsets when no line is available", () => {
  const hash = formatSourceSelectionHash({
    file: "main.tex",
    startUtf8: 19
  });

  assert.equal(hash, "#src=main.tex&offset=19");
  assert.deepEqual(parseSourceHashRequest(hash), {
    file: "main.tex",
    offset: 19,
    line: null
  });
  assert.equal(parseSourceHashRequest("#zoom=120"), null);
});

test("source selection hash prefers canonical server-provided hashes", () => {
  assert.equal(
    formatSourceSelectionHash({
      file: "sections/intro.tex",
      startLine: 7,
      startUtf8: 42,
      sourceHash: "#src=sections%2Fintro.tex&line=8&column=2"
    }),
    "#src=sections%2Fintro.tex&line=8&column=2"
  );
});

test("source request from selection prefers line information when present", () => {
  assert.deepEqual(sourceRequestFromSelection({
    file: "sections/intro.tex",
    startLine: 9,
    startColumn: 3,
    startUtf8: 120
  }), {
    file: "sections/intro.tex",
    offset: null,
    line: 9,
    column: 3
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "main.tex",
    startLine: 9.2,
    startUtf8: 120,
    sourceHash: "#zoom=120"
  }), {
    file: "main.tex",
    offset: null,
    line: 9
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "sections/intro.tex",
    startLine: 9,
    startUtf8: 120,
    sourceHash: "#src=sections%2Fintro.tex&line=9&column=5"
  }), {
    file: "sections/intro.tex",
    offset: null,
    line: 9,
    column: 5
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "main.tex",
    startLine: 9.2,
    startUtf8: 120,
    sourceHash: "#src=main.tex&line=9"
  }), {
    file: "main.tex",
    offset: null,
    line: 9
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "main.tex",
    startLine: 9.2,
    startUtf8: 120,
    sourceHash: "#src=main.tex&offset=12"
  }), {
    file: "main.tex",
    offset: null,
    line: 9
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "main.tex",
    startLine: 9.2,
    startUtf8: 120,
    sourceHash: "#src=main.tex&line=9&column=5"
  }), {
    file: "main.tex",
    offset: null,
    line: 9,
    column: 5
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "main.tex",
    startLine: 0,
    startUtf8: 120,
    sourceHash: "#src=main.tex&line=1&column=5"
  }), {
    file: "main.tex",
    offset: null,
    line: 1,
    column: 5
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "main.tex",
    startLine: 9.2,
    startUtf8: 120,
    sourceHash: "#src=main.tex&line=10&column=5"
  }), {
    file: "main.tex",
    offset: null,
    line: 9
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "main.tex",
    startLine: 0,
    startUtf8: 120,
    sourceHash: "#src=main.tex&line=2&column=5"
  }), {
    file: "main.tex",
    offset: null,
    line: 1
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "sections/intro.tex",
    startLine: 9.2,
    startUtf8: 120,
    sourceHash: "src=sections%2Fintro.tex&line=9"
  }), {
    file: "sections/intro.tex",
    offset: null,
    line: 9
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "sections/intro.tex",
    startLine: 9.2,
    startUtf8: 120,
    sourceHash: "#src=sections%2Fintro.tex&line=9&column=5"
  }), {
    file: "sections/intro.tex",
    offset: null,
    line: 9,
    column: 5
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "sections/intro.tex",
    startLine: 9.2,
    startUtf8: 120,
    sourceHash: "src=sections%2Fintro.tex&line=9&column=5"
  }), {
    file: "sections/intro.tex",
    offset: null,
    line: 9,
    column: 5
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "sections/intro.tex",
    startLine: 0,
    startUtf8: 120,
    sourceHash: "#src=sections%2Fintro.tex&line=1&column=5"
  }), {
    file: "sections/intro.tex",
    offset: null,
    line: 1,
    column: 5
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "sections/intro.tex",
    startLine: 9.2,
    startUtf8: 120,
    sourceHash: "src=sections%2Fintro.tex&line=10&column=5"
  }), {
    file: "sections/intro.tex",
    offset: null,
    line: 9
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "sections/intro.tex",
    startLine: 9.2,
    startUtf8: 120,
    sourceHash: "#src=main.tex&line=9&column=5"
  }), {
    file: "sections/intro.tex",
    offset: null,
    line: 9
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "main.tex",
    startLine: 9.2,
    startUtf8: 120,
    sourceHash: "#src=main.tex&line=9&column=1"
  }), {
    file: "main.tex",
    offset: null,
    line: 9,
    column: 1
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "main.tex",
    startLine: 0,
    startUtf8: 120,
    sourceHash: "src=main.tex&line=1&column=1"
  }), {
    file: "main.tex",
    offset: null,
    line: 1,
    column: 1
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "main.tex",
    startLine: 0,
    startColumn: 0,
    startUtf8: 120,
    sourceHash: "#src=main.tex&line=1&column=5"
  }), {
    file: "main.tex",
    offset: null,
    line: 1,
    column: 1
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "sections/intro.tex",
    startLine: 9.2,
    startColumn: 4.7,
    startUtf8: 120,
    sourceHash: "src=sections%2Fintro.tex&line=9&column=1"
  }), {
    file: "sections/intro.tex",
    offset: null,
    line: 9,
    column: 5
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "main.tex",
    startLine: 9.2,
    startColumn: 0,
    startUtf8: 120,
    sourceHash: "#src=main.tex&line=10&column=5"
  }), {
    file: "main.tex",
    offset: null,
    line: 9,
    column: 1
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "sections/intro.tex",
    startLine: 9.2,
    startColumn: 4.7,
    startUtf8: 120,
    sourceHash: "#src=main.tex&line=9&column=1"
  }), {
    file: "sections/intro.tex",
    offset: null,
    line: 9,
    column: 5
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "main.tex",
    startUtf8: 31
  }), {
    file: "main.tex",
    offset: 31,
    line: null
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "main.tex",
    startLine: 3.2,
    startColumn: 4.7,
    startUtf8: 12.7
  }), {
    file: "main.tex",
    offset: null,
    line: 3,
    column: 5
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "main.tex",
    startLine: 0,
    startColumn: 0,
    startUtf8: 12.7
  }), {
    file: "main.tex",
    offset: null,
    line: 1,
    column: 1
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "main.tex",
    startUtf8: 12.7
  }), {
    file: "main.tex",
    offset: 13,
    line: null
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "main.tex",
    startUtf8: 12.7,
    sourceHash: "#src=main.tex&line=9&column=5"
  }), {
    file: "main.tex",
    offset: 13,
    line: null
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "main.tex",
    startUtf8: 12.7,
    sourceHash: "#src=main.tex&line=9"
  }), {
    file: "main.tex",
    offset: 13,
    line: null
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "main.tex",
    startUtf8: 12.7,
    sourceHash: "#zoom=120"
  }), {
    file: "main.tex",
    offset: 13,
    line: null
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "main.tex",
    startUtf8: 12.7,
    sourceHash: "#src=main.tex&offset=12"
  }), {
    file: "main.tex",
    offset: 13,
    line: null
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "main.tex",
    startUtf8: -0.4
  }), {
    file: "main.tex",
    offset: 0,
    line: null
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "sections/intro.tex",
    startUtf8: 12.7,
    sourceHash: "src=sections%2Fintro.tex&offset=9"
  }), {
    file: "sections/intro.tex",
    offset: 13,
    line: null
  });
  assert.deepEqual(sourceRequestFromSelection({
    file: "sections/intro.tex",
    startUtf8: -0.4,
    sourceHash: "src=sections%2Fintro.tex&offset=9"
  }), {
    file: "sections/intro.tex",
    offset: 0,
    line: null
  });
  assert.equal(sourceRequestFromSelection(null), null);
});

test("full preview refresh clears stale sync selection state", () => {
  let state = reduce(initialState, {
    type: "full_pdf_ready",
    rev: 9,
    pdf_url: "/artifacts/rev/9/main.pdf",
    page_ids: ["page-a"],
    page_artifacts: [
      { page_id: "page-a", pdf_url: "/artifacts/rev/9/pages/page-a.pdf", svg_url: "/artifacts/rev/9/pages/page-a.svg" }
    ]
  });
  state = reduce(state, {
    type: "ui_syncmap_ready",
    rev: 9,
    page_id: "page-a",
    page_width_px: 612,
    page_height_px: 792,
    page_source_start_utf8: 0,
    page_source_end_utf8: 10,
    page_output_start_utf8: 0,
    page_output_end_utf8: 24,
    items: [{ file: "main.tex", start_utf8: 0, end_utf8: 10, start_line: 1, end_line: 2, left_px: 72, right_px: 144, top_px: 0, bottom_px: 100 }]
  });
  state = reduce(state, {
    type: "ui_sync_selected",
    item: {
      pageId: "page-a",
      pageHeightPx: 792,
      file: "main.tex",
      startUtf8: 0,
      endUtf8: 10,
      startLine: 1,
      endLine: 2,
      topPx: 0,
      bottomPx: 100
    }
  });
  state = reduce(state, {
    type: "source_snapshot",
    rev: 9,
    files: [
      { file: "main.tex", content: "lead\nbody\n", line_count: 2 }
    ]
  });
  state = reduce(state, {
    type: "full_pdf_ready",
    rev: 10,
    pdf_url: "/artifacts/rev/10/main.pdf",
    page_ids: ["page-b"],
    page_artifacts: [
      { page_id: "page-b", pdf_url: "/artifacts/rev/10/pages/page-b.pdf", svg_url: "/artifacts/rev/10/pages/page-b.svg" }
    ]
  });

  assert.deepEqual(state.syncMaps, {});
  assert.deepEqual(state.sourceFiles, {});
  assert.equal(state.selectedSource, null);
  assert.equal(state.hoveredSource, null);
});

test("source jump result focuses the matching page and source band", () => {
  let state = reduce(initialState, {
    type: "full_pdf_ready",
    rev: 11,
    pdf_url: "/artifacts/rev/11/main.pdf",
    page_ids: ["page-a", "page-b"],
    page_artifacts: [
      { page_id: "page-a", pdf_url: "/artifacts/rev/11/pages/page-a.pdf", svg_url: "/artifacts/rev/11/pages/page-a.svg" },
      { page_id: "page-b", pdf_url: "/artifacts/rev/11/pages/page-b.pdf", svg_url: "/artifacts/rev/11/pages/page-b.svg" }
    ]
  });
  state = reduce(state, {
    type: "ui_source_jump_resolved",
    page_id: "page-b",
    page_index: 1,
    item: {
      pageId: "page-b",
      pageHeightPx: 792,
      file: "main.tex",
      startUtf8: 5,
      endUtf8: 14,
      startLine: 2,
      endLine: 3,
      topPx: 100,
      bottomPx: 220
    }
  });

  assert.equal(state.currentPage, 2);
  assert.equal(state.hoveredSource, null);
  assert.deepEqual(state.selectedSource, {
    pageId: "page-b",
    pageHeightPx: 792,
    file: "main.tex",
    startUtf8: 5,
    endUtf8: 14,
    startLine: 2,
    endLine: 3,
    topPx: 100,
    bottomPx: 220
  });
});

test("source hover result focuses the matching page without clearing selection", () => {
  let state = reduce(initialState, {
    type: "full_pdf_ready",
    rev: 12,
    pdf_url: "/artifacts/rev/12/main.pdf",
    page_ids: ["page-a", "page-b"],
    page_artifacts: [
      { page_id: "page-a", pdf_url: "/artifacts/rev/12/pages/page-a.pdf", svg_url: "/artifacts/rev/12/pages/page-a.svg" },
      { page_id: "page-b", pdf_url: "/artifacts/rev/12/pages/page-b.pdf", svg_url: "/artifacts/rev/12/pages/page-b.svg" }
    ]
  });
  state = reduce(state, {
    type: "ui_sync_selected",
    item: {
      pageId: "page-a",
      pageHeightPx: 792,
      file: "main.tex",
      startUtf8: 0,
      endUtf8: 4,
      startLine: 1,
      endLine: 1,
      topPx: 0,
      bottomPx: 80
    }
  });
  state = reduce(state, {
    type: "ui_source_hover_resolved",
    page_id: "page-b",
    page_index: 1,
    item: {
      pageId: "page-b",
      pageHeightPx: 792,
      file: "main.tex",
      startUtf8: 5,
      endUtf8: 14,
      startLine: 2,
      endLine: 3,
      topPx: 100,
      bottomPx: 220
    }
  });

  assert.equal(state.currentPage, 2);
  assert.deepEqual(state.selectedSource, {
    pageId: "page-a",
    pageHeightPx: 792,
    file: "main.tex",
    startUtf8: 0,
    endUtf8: 4,
    startLine: 1,
    endLine: 1,
    topPx: 0,
    bottomPx: 80
  });
  assert.deepEqual(state.hoveredSource, {
    pageId: "page-b",
    pageHeightPx: 792,
    file: "main.tex",
    startUtf8: 5,
    endUtf8: 14,
    startLine: 2,
    endLine: 3,
    topPx: 100,
    bottomPx: 220
  });
});

test("source jump result preserves page-local source and output windows", () => {
  const state = reduce(
    reduce(initialState, {
      type: "full_pdf_ready",
      rev: 14,
      pdf_url: "/artifacts/rev/14/main.pdf",
      page_ids: ["page-a"],
      page_artifacts: [
        { page_id: "page-a", pdf_url: "/artifacts/rev/14/pages/page-a.pdf", svg_url: "/artifacts/rev/14/pages/page-a.svg" }
      ]
    }),
    {
      type: "ui_source_jump_resolved",
      page_id: "page-a",
      page_index: 0,
      item: {
        pageId: "page-a",
        pageHeightPx: 792,
        pageSourceStartUtf8: 5,
        pageSourceEndUtf8: 20,
        pageOutputStartUtf8: 32,
        pageOutputEndUtf8: 64,
        outputStartUtf8: 40,
        outputEndUtf8: 56,
        file: "main.tex",
        startUtf8: 8,
        endUtf8: 14,
        startLine: 2,
        endLine: 3,
        topPx: 100,
        bottomPx: 220
      }
    }
  );

  assert.deepEqual(state.selectedSource, {
    pageId: "page-a",
    pageHeightPx: 792,
    pageSourceStartUtf8: 5,
    pageSourceEndUtf8: 20,
    pageOutputStartUtf8: 32,
    pageOutputEndUtf8: 64,
    outputStartUtf8: 40,
    outputEndUtf8: 56,
    file: "main.tex",
    startUtf8: 8,
    endUtf8: 14,
    startLine: 2,
    endLine: 3,
    topPx: 100,
    bottomPx: 220
  });
});

test("jump-context helper preserves page-local source and output windows", () => {
  assert.deepEqual(
    syncSelectionFromJumpContext({
      page_id: "page-a",
      source_hash: "#src=main.tex&line=2&column=4",
      page_width_px: 612,
      page_height_px: 792,
      page_source_start_utf8: 5,
      page_source_end_utf8: 20,
      page_output_start_utf8: 32,
      page_output_end_utf8: 64,
      item: {
        file: "main.tex",
        start_utf8: 8,
        end_utf8: 14,
        output_start_utf8: 40,
        output_end_utf8: 56,
        start_line: 2,
        end_line: 3,
        left_px: 72,
        right_px: 180,
        top_px: 100,
        bottom_px: 220
      }
    }),
    {
      itemId: "page-a:main.tex:8:14:2:3",
      pageId: "page-a",
      pageWidthPx: 612,
      pageHeightPx: 792,
      pageSourceStartUtf8: 5,
      pageSourceEndUtf8: 20,
      pageOutputStartUtf8: 32,
      pageOutputEndUtf8: 64,
      sourceHash: "#src=main.tex&line=2&column=4",
      outputStartUtf8: 40,
      outputEndUtf8: 56,
      file: "main.tex",
      startUtf8: 8,
      endUtf8: 14,
      startLine: 2,
      endLine: 3,
      leftPx: 72,
      rightPx: 180,
      topPx: 100,
      bottomPx: 220
    }
  );
});

test("resolved source request detail keeps canonical response hash and item context", () => {
  const detail = resolvedSourceRequestDetail(
    15,
    { file: "main.tex", offset: 8, line: null },
    {
      page_id: "page-a",
      source_hash: "#src=main.tex&line=2&column=4",
      absolute_file: "/tmp/project/main.tex",
      file_uri: "file:///tmp/project/main.tex",
      editor_uri: "vscode://file/tmp/project/main.tex:2:4",
      editor_preview_kind: "command_and_uri",
      line: 2,
      line0: 1,
      column: 4,
      column0: 3,
      editor_cwd: "/tmp/project",
      editor_launch_supported: true,
      editor_program: "/bin/true",
      editor_args: ["main.tex"],
      editor_command_line: "/bin/true main.tex",
      launched: true,
      page_width_px: 612,
      page_height_px: 792,
      page_source_start_utf8: 5,
      page_source_end_utf8: 20,
      page_output_start_utf8: 32,
      page_output_end_utf8: 64,
      item: {
        file: "main.tex",
        start_utf8: 8,
        end_utf8: 14,
        output_start_utf8: 40,
        output_end_utf8: 56,
        start_line: 2,
        end_line: 3,
        left_px: 72,
        right_px: 180,
        top_px: 100,
        bottom_px: 220
      }
    }
  );

  assert.equal(detail.rev, 15);
  assert.deepEqual(detail.request, { file: "main.tex", offset: 8, line: null });
  assert.equal(detail.sourceHash, "#src=main.tex&line=2&column=4");
  assert.equal(detail.item?.sourceHash, "#src=main.tex&line=2&column=4");
  assert.equal(detail.absoluteFile, "/tmp/project/main.tex");
  assert.equal(detail.fileUri, "file:///tmp/project/main.tex");
  assert.equal(detail.editorUri, "vscode://file/tmp/project/main.tex:2:4");
  assert.equal(detail.editorPreviewKind, "command_and_uri");
  assert.equal(detail.line, 2);
  assert.equal(detail.line0, 1);
  assert.equal(detail.column, 4);
  assert.equal(detail.column0, 3);
  assert.equal(detail.editorCwd, "/tmp/project");
  assert.equal(detail.editorLaunchSupported, true);
  assert.equal(detail.editorProgram, "/bin/true");
  assert.deepEqual(detail.editorArgs, ["main.tex"]);
  assert.equal(detail.editorCommandLine, "/bin/true main.tex");
  assert.equal(detail.launched, true);
  assert.equal(detail.launchRequested, true);
  assert.equal(detail.previewOnly, false);
});

test("resolved source request detail marks preview-only open-source requests", () => {
  const detail = resolvedSourceRequestDetail(
    15,
    { file: "main.tex", offset: 8, line: null, launch: false },
    {
      page_id: "page-a",
      source_hash: "#src=main.tex&line=2&column=4",
      absolute_file: "/tmp/project/main.tex",
      file_uri: "file:///tmp/project/main.tex",
      editor_uri: "",
      editor_preview_kind: "none",
      line: 2,
      line0: 1,
      column: 4,
      column0: 3,
      editor_cwd: "/tmp/project",
      editor_launch_supported: false,
      editor_program: "",
      editor_args: [],
      editor_command_line: "",
      launched: false,
      item: null
    }
  );

  assert.equal(detail.sourceHash, "#src=main.tex&line=2&column=4");
  assert.equal(detail.absoluteFile, "/tmp/project/main.tex");
  assert.equal(detail.fileUri, "file:///tmp/project/main.tex");
  assert.equal(detail.editorUri, "");
  assert.equal(detail.editorPreviewKind, "none");
  assert.equal(detail.line, 2);
  assert.equal(detail.line0, 1);
  assert.equal(detail.column, 4);
  assert.equal(detail.column0, 3);
  assert.equal(detail.editorCwd, "/tmp/project");
  assert.equal(detail.editorLaunchSupported, false);
  assert.equal(detail.editorCommandLine, "");
  assert.equal(detail.launched, false);
  assert.equal(detail.launchRequested, false);
  assert.equal(detail.previewOnly, true);
});

test("resolved source request detail falls back to the source selection hash without an item", () => {
  const detail = resolvedSourceRequestDetail(
    15,
    { file: "main.tex", offset: null, line: 3 },
    {
      page_id: null,
      item: null,
      absolute_file: "",
      file_uri: "",
      editor_uri: "",
      editor_preview_kind: "none",
      line: 3,
      line0: 2,
      column: 1,
      column0: 0,
      editor_cwd: "",
      editor_launch_supported: false,
      source_hash: ""
    },
    {
      file: "main.tex",
      startLine: 3,
      startUtf8: 10
    }
  );

  assert.equal(detail.item, null);
  assert.equal(detail.sourceHash, "#src=main.tex&line=3");
  assert.equal(detail.absoluteFile, "");
  assert.equal(detail.fileUri, "");
  assert.equal(detail.editorUri, "");
  assert.equal(detail.editorPreviewKind, "none");
  assert.equal(detail.line, 3);
  assert.equal(detail.line0, 2);
  assert.equal(detail.column, 1);
  assert.equal(detail.column0, 0);
  assert.equal(detail.editorCwd, "");
  assert.equal(detail.editorLaunchSupported, false);
});

test("resolved source request detail falls back to request source hash when response hash is missing", () => {
  const detail = resolvedSourceRequestDetail(
    15,
    { file: "main.tex", offset: 8, line: null, source_hash: "#src=main.tex&line=2&column=4" },
    {
      page_id: "page-a",
      source_hash: "",
      absolute_file: "/tmp/project/main.tex",
      file_uri: "file:///tmp/project/main.tex",
      editor_uri: "",
      line: 2,
      line0: 1,
      column: 4,
      column0: 3,
      editor_cwd: "/tmp/project",
      editor_launch_supported: false,
      editor_program: "",
      editor_args: [],
      editor_command_line: "",
      launched: false,
      item: {
        file: "main.tex",
        start_utf8: 8,
        end_utf8: 14,
        output_start_utf8: 40,
        output_end_utf8: 56,
        start_line: 2,
        end_line: 3,
        left_px: 72,
        right_px: 180,
        top_px: 100,
        bottom_px: 220
      }
    }
  );

  assert.equal(detail.sourceHash, "#src=main.tex&line=2&column=4");
});

test("open source result focuses the matching page and preserves page-local windows", () => {
  const state = reduce(
    reduce(initialState, {
      type: "full_pdf_ready",
      rev: 15,
      pdf_url: "/artifacts/rev/15/main.pdf",
      page_ids: ["page-a", "page-b"],
      page_artifacts: [
        { page_id: "page-a", pdf_url: "/artifacts/rev/15/pages/page-a.pdf", svg_url: "/artifacts/rev/15/pages/page-a.svg" },
        { page_id: "page-b", pdf_url: "/artifacts/rev/15/pages/page-b.pdf", svg_url: "/artifacts/rev/15/pages/page-b.svg" }
      ]
    }),
    {
      type: "ui_open_source_resolved",
      page_id: "page-b",
      page_index: 1,
      item: {
        pageId: "page-b",
        pageWidthPx: 612,
        pageHeightPx: 792,
        sourceHash: "#src=main.tex&line=2&column=4",
        pageSourceStartUtf8: 5,
        pageSourceEndUtf8: 20,
        pageOutputStartUtf8: 32,
        pageOutputEndUtf8: 64,
        outputStartUtf8: 40,
        outputEndUtf8: 56,
        file: "main.tex",
        startUtf8: 8,
        endUtf8: 14,
        startLine: 2,
        endLine: 3,
        topPx: 100,
        bottomPx: 220
      }
    }
  );

  assert.equal(state.currentPage, 2);
  assert.equal(state.hoveredSource, null);
  assert.deepEqual(state.selectedSource, {
    pageId: "page-b",
    pageWidthPx: 612,
    pageHeightPx: 792,
    sourceHash: "#src=main.tex&line=2&column=4",
    pageSourceStartUtf8: 5,
    pageSourceEndUtf8: 20,
    pageOutputStartUtf8: 32,
    pageOutputEndUtf8: 64,
    outputStartUtf8: 40,
    outputEndUtf8: 56,
    file: "main.tex",
    startUtf8: 8,
    endUtf8: 14,
    startLine: 2,
    endLine: 3,
    topPx: 100,
    bottomPx: 220
  });
});

test("full document fallback keeps no page artifacts when page ids are absent", () => {
  const state = reduce(initialState, {
    type: "full_pdf_ready",
    rev: 6,
    pdf_url: "/artifacts/rev/6/main.pdf",
    page_ids: [],
    page_artifacts: []
  });

  assert.equal(state.pdfUrl, "/artifacts/rev/6/main.pdf");
  assert.deepEqual(state.pages, []);
  assert.deepEqual(state.pageIds, []);
});

test("tile manifest installs page-specific tiles for the current zoom bucket", () => {
  let state = reduce(initialState, {
    type: "full_pdf_ready",
    rev: 7,
    pdf_url: "/artifacts/rev/7/main.pdf",
    page_ids: ["page-a", "page-b"],
    page_artifacts: [
      { page_id: "page-a", pdf_url: "/artifacts/rev/7/pages/page-a.pdf", svg_url: "/artifacts/rev/7/pages/page-a.svg" },
      { page_id: "page-b", pdf_url: "/artifacts/rev/7/pages/page-b.pdf", svg_url: "/artifacts/rev/7/pages/page-b.svg" }
    ]
  });
  state = reduce(state, { type: "ui_page_changed", page: 2 });
  state = reduce(state, { type: "ui_zoom_changed", zoom: 1.2 });
  state = reduce(state, {
    type: "ui_tiles_ready",
    rev: 7,
    page_id: "page-b",
    zoom_bucket: 120,
    tile_size: 256,
    items: [
      {
        page_id: "page-b",
        zoom_bucket: 120,
        tile_x: 0,
        tile_y: 0,
        png_url: "/artifacts/rev/7/tiles/page-b/120/0/0.png"
      }
    ]
  });

  assert.deepEqual(state.tileLayers, {
    "page-b": {
      zoomBucket: 120,
      tileSize: 256,
      items: [
        {
          page_id: "page-b",
          zoom_bucket: 120,
          tile_x: 0,
          tile_y: 0,
          png_url: "/artifacts/rev/7/tiles/page-b/120/0/0.png"
        }
      ]
    }
  });
});

test("tile manifests accumulate across multiple visible pages", () => {
  let state = reduce(initialState, {
    type: "full_pdf_ready",
    rev: 8,
    pdf_url: "/artifacts/rev/8/main.pdf",
    page_ids: ["page-a", "page-b"],
    page_artifacts: [
      { page_id: "page-a", pdf_url: "/artifacts/rev/8/pages/page-a.pdf", svg_url: "/artifacts/rev/8/pages/page-a.svg" },
      { page_id: "page-b", pdf_url: "/artifacts/rev/8/pages/page-b.pdf", svg_url: "/artifacts/rev/8/pages/page-b.svg" }
    ]
  });
  state = reduce(state, {
    type: "ui_tiles_ready",
    rev: 8,
    page_id: "page-a",
    zoom_bucket: 100,
    tile_size: 256,
    items: [{ page_id: "page-a", zoom_bucket: 100, tile_x: 0, tile_y: 0, png_url: "/tile-a.png" }]
  });
  state = reduce(state, {
    type: "ui_tiles_ready",
    rev: 8,
    page_id: "page-b",
    zoom_bucket: 100,
    tile_size: 256,
    items: [{ page_id: "page-b", zoom_bucket: 100, tile_x: 1, tile_y: 0, png_url: "/tile-b.png" }]
  });

  assert.deepEqual(state.tileLayers, {
    "page-a": {
      zoomBucket: 100,
      tileSize: 256,
      items: [{ page_id: "page-a", zoom_bucket: 100, tile_x: 0, tile_y: 0, png_url: "/tile-a.png" }]
    },
    "page-b": {
      zoomBucket: 100,
      tileSize: 256,
      items: [{ page_id: "page-b", zoom_bucket: 100, tile_x: 1, tile_y: 0, png_url: "/tile-b.png" }]
    }
  });
});

test("tile manifests merge new tiles into an existing page cache", () => {
  let state = reduce(initialState, {
    type: "full_pdf_ready",
    rev: 8,
    pdf_url: "/artifacts/rev/8/main.pdf",
    page_ids: ["page-a"],
    page_artifacts: [
      { page_id: "page-a", pdf_url: "/artifacts/rev/8/pages/page-a.pdf", svg_url: "/artifacts/rev/8/pages/page-a.svg" }
    ]
  });
  state = reduce(state, {
    type: "ui_tiles_ready",
    rev: 8,
    page_id: "page-a",
    zoom_bucket: 100,
    tile_size: 256,
    items: [{ page_id: "page-a", zoom_bucket: 100, tile_x: 0, tile_y: 0, png_url: "/tile-a-0.png" }]
  });
  state = reduce(state, {
    type: "ui_tiles_ready",
    rev: 8,
    page_id: "page-a",
    zoom_bucket: 100,
    tile_size: 256,
    items: [{ page_id: "page-a", zoom_bucket: 100, tile_x: 1, tile_y: 0, png_url: "/tile-a-1.png" }]
  });

  assert.deepEqual(state.tileLayers, {
    "page-a": {
      zoomBucket: 100,
      tileSize: 256,
      items: [
        { page_id: "page-a", zoom_bucket: 100, tile_x: 0, tile_y: 0, png_url: "/tile-a-0.png" },
        { page_id: "page-a", zoom_bucket: 100, tile_x: 1, tile_y: 0, png_url: "/tile-a-1.png" }
      ]
    }
  });
});

test("full preview refresh retains caches for unchanged page artifacts", () => {
  let state = reduce(initialState, {
    type: "full_pdf_ready",
    rev: 8,
    pdf_url: "/artifacts/rev/8/main.pdf",
    page_ids: ["page-a", "page-b"],
    page_artifacts: [
      { page_id: "page-a", pdf_url: "/artifacts/rev/8/pages/page-a.pdf", svg_url: "/artifacts/rev/8/pages/page-a.svg" },
      { page_id: "page-b", pdf_url: "/artifacts/rev/8/pages/page-b.pdf", svg_url: "/artifacts/rev/8/pages/page-b.svg" }
    ]
  });
  state = reduce(state, {
    type: "ui_syncmap_ready",
    rev: 8,
    page_id: "page-a",
    page_width_px: 612,
    page_height_px: 792,
    page_source_start_utf8: 0,
    page_source_end_utf8: 20,
    page_output_start_utf8: 0,
    page_output_end_utf8: 24,
    items: [{ file: "main.tex", start_utf8: 0, end_utf8: 10, start_line: 1, end_line: 2, left_px: 72, right_px: 144, top_px: 0, bottom_px: 100 }]
  });
  state = reduce(state, {
    type: "ui_tiles_ready",
    rev: 8,
    page_id: "page-a",
    zoom_bucket: 100,
    tile_size: 256,
    items: [{ page_id: "page-a", zoom_bucket: 100, tile_x: 0, tile_y: 0, png_url: "/tile-a.png" }]
  });
  state = reduce(state, {
    type: "ui_sync_selected",
    item: {
      pageId: "page-a",
      file: "main.tex",
      startUtf8: 0,
      endUtf8: 10,
      startLine: 1,
      endLine: 2
    }
  });

  state = reduce(state, {
    type: "full_pdf_ready",
    rev: 9,
    pdf_url: "/artifacts/rev/9/main.pdf",
    page_ids: ["page-a", "page-b"],
    page_artifacts: [
      { page_id: "page-a", pdf_url: "/artifacts/rev/8/pages/page-a.pdf", svg_url: "/artifacts/rev/8/pages/page-a.svg" },
      { page_id: "page-b", pdf_url: "/artifacts/rev/9/pages/page-b.pdf", svg_url: "/artifacts/rev/9/pages/page-b.svg" }
    ]
  });

  assert.deepEqual(state.tileLayers, {
    "page-a": {
      zoomBucket: 100,
      tileSize: 256,
      items: [{ page_id: "page-a", zoom_bucket: 100, tile_x: 0, tile_y: 0, png_url: "/tile-a.png" }]
    }
  });
  assert.equal(state.syncMaps["page-a"]?.rev, 9);
  assert.equal(state.selectedSource?.pageId, "page-a");
  assert.equal(state.hoveredSource, null);
});

test("page patches retain caches for untouched pages", () => {
  let state = reduce(initialState, {
    type: "full_pdf_ready",
    rev: 8,
    pdf_url: "/artifacts/rev/8/main.pdf",
    page_ids: ["page-a", "page-b"],
    page_artifacts: [
      { page_id: "page-a", pdf_url: "/artifacts/rev/8/pages/page-a.pdf", svg_url: "/artifacts/rev/8/pages/page-a.svg" },
      { page_id: "page-b", pdf_url: "/artifacts/rev/8/pages/page-b.pdf", svg_url: "/artifacts/rev/8/pages/page-b.svg" }
    ]
  });
  state = reduce(state, {
    type: "ui_syncmap_ready",
    rev: 8,
    page_id: "page-a",
    page_width_px: 612,
    page_height_px: 792,
    page_source_start_utf8: 0,
    page_source_end_utf8: 20,
    page_output_start_utf8: 0,
    page_output_end_utf8: 24,
    items: [{ file: "main.tex", start_utf8: 0, end_utf8: 10, start_line: 1, end_line: 2, left_px: 72, right_px: 144, top_px: 0, bottom_px: 100 }]
  });
  state = reduce(state, {
    type: "ui_tiles_ready",
    rev: 8,
    page_id: "page-a",
    zoom_bucket: 100,
    tile_size: 256,
    items: [{ page_id: "page-a", zoom_bucket: 100, tile_x: 0, tile_y: 0, png_url: "/tile-a.png" }]
  });
  state = reduce(state, {
    type: "ui_sync_selected",
    item: {
      pageId: "page-a",
      file: "main.tex",
      startUtf8: 0,
      endUtf8: 10,
      startLine: 1,
      endLine: 2
    }
  });

  state = reduce(state, {
    type: "patch_pages",
    rev: 9,
    ops: [{
      op: "replace_page",
      index: 1,
      page_id: "page-c",
      pdf_url: "/artifacts/rev/9/pages/page-c.pdf",
      svg_url: "/artifacts/rev/9/pages/page-c.svg"
    }]
  });

  assert.deepEqual(state.pageIds, ["page-a", "page-c"]);
  assert.deepEqual(state.tileLayers, {
    "page-a": {
      zoomBucket: 100,
      tileSize: 256,
      items: [{ page_id: "page-a", zoom_bucket: 100, tile_x: 0, tile_y: 0, png_url: "/tile-a.png" }]
    }
  });
  assert.equal(state.syncMaps["page-a"]?.rev, 9);
  assert.equal(state.selectedSource?.pageId, "page-a");
  assert.equal(state.syncMaps["page-b"], undefined);
  assert.equal(state.tileLayers["page-b"], undefined);
});

test("stale tile manifests are ignored and zoom changes clear tile layers", () => {
  let state = reduce(initialState, {
    type: "full_pdf_ready",
    rev: 8,
    pdf_url: "/artifacts/rev/8/main.pdf",
    page_ids: ["page-a", "page-b"],
    page_artifacts: [
      { page_id: "page-a", pdf_url: "/artifacts/rev/8/pages/page-a.pdf", svg_url: "/artifacts/rev/8/pages/page-a.svg" },
      { page_id: "page-b", pdf_url: "/artifacts/rev/8/pages/page-b.pdf", svg_url: "/artifacts/rev/8/pages/page-b.svg" }
    ]
  });
  state = reduce(state, {
    type: "ui_tiles_ready",
    rev: 7,
    page_id: "page-a",
    zoom_bucket: 100,
    tile_size: 256,
    items: [{ page_id: "page-a", zoom_bucket: 100, tile_x: 0, tile_y: 0, png_url: "/tile-old.png" }]
  });
  assert.deepEqual(state.tileLayers, {});

  state = reduce(state, {
    type: "ui_tiles_ready",
    rev: 8,
    page_id: "page-missing",
    zoom_bucket: 100,
    tile_size: 256,
    items: [{ page_id: "page-missing", zoom_bucket: 100, tile_x: 0, tile_y: 0, png_url: "/tile-missing.png" }]
  });
  assert.deepEqual(state.tileLayers, {});

  state = reduce(state, {
    type: "ui_tiles_ready",
    rev: 8,
    page_id: "page-a",
    zoom_bucket: 100,
    tile_size: 256,
    items: [{ page_id: "page-a", zoom_bucket: 100, tile_x: 0, tile_y: 0, png_url: "/tile-a.png" }]
  });
  assert.deepEqual(state.tileLayers, {
    "page-a": {
      zoomBucket: 100,
      tileSize: 256,
      items: [{ page_id: "page-a", zoom_bucket: 100, tile_x: 0, tile_y: 0, png_url: "/tile-a.png" }]
    }
  });

  state = reduce(state, { type: "ui_zoom_changed", zoom: 1.5 });
  assert.deepEqual(state.tileLayers, {});

  state = reduce(state, {
    type: "ui_tiles_ready",
    rev: 8,
    page_id: "page-a",
    zoom_bucket: 100,
    tile_size: 256,
    items: [{ page_id: "page-a", zoom_bucket: 100, tile_x: 0, tile_y: 0, png_url: "/tile-stale.png" }]
  });
  assert.deepEqual(state.tileLayers, {});
});
