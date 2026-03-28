import test from "node:test";
import assert from "node:assert/strict";
import { pathToFileURL } from "node:url";

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
} from "./app.mjs";

async function withViewerBrowserHarness(optionsOrRun, maybeRun) {
  const options = typeof optionsOrRun === "function" ? {} : (optionsOrRun ?? {});
  const run = typeof optionsOrRun === "function" ? optionsOrRun : maybeRun;
  const previousWindow = globalThis.window;
  const previousDocument = globalThis.document;
  const previousFetch = globalThis.fetch;
  const previousWebSocket = globalThis.WebSocket;
  const previousCustomEvent = globalThis.CustomEvent;
  const moduleUrl = `${pathToFileURL("/home/seorii/dev/hancomac/latexd/web/viewer/app.mjs").href}?browser-test=${Date.now()}-${Math.random()}`;

  class FakeNode extends EventTarget {
    constructor(tagName) {
      super();
      this.tagName = String(tagName || "div").toUpperCase();
      this.children = [];
      this.parentNode = null;
      this.className = "";
      this.dataset = {};
      this.style = {};
      this.hidden = false;
      this.disabled = false;
      this.href = "";
      this.src = "";
      this.type = "";
      this.alt = "";
      this.textContent = "";
      this.scrollTop = 0;
      this.naturalWidth = 0;
      this.naturalHeight = 0;
      this._innerHTML = "";
    }

    append(...children) {
      this.appendChild(...children);
    }

    appendChild(...children) {
      for (const child of children) {
        if (!child) {
          continue;
        }
        if (child.tagName === "#FRAGMENT") {
          this.appendChild(...child.children);
          continue;
        }
        child.parentNode = this;
        this.children.push(child);
      }
      return children[children.length - 1] ?? null;
    }

    replaceChildren(...children) {
      this.children = [];
      this.appendChild(...children);
    }

    setAttribute(name, value) {
      this[name] = String(value);
    }

    removeAttribute(name) {
      delete this[name];
    }

    scrollIntoView() {}

    getBoundingClientRect() {
      return {
        left: 0,
        top: 0,
        right: 612,
        bottom: 792,
        width: 612,
        height: 792
      };
    }

    querySelector(selector) {
      if (!selector.startsWith(".")) {
        return null;
      }
      const wanted = selector.slice(1);
      const queue = [...this.children];
      while (queue.length > 0) {
        const node = queue.shift();
        if (String(node.className).split(/\s+/).includes(wanted)) {
          return node;
        }
        queue.push(...node.children);
      }
      return null;
    }

    set innerHTML(html) {
      this._innerHTML = html;
      this.children = [];
      if (html.includes("page-card__meta")) {
        const meta = new FakeNode("div");
        meta.className = "page-card__meta";
        const label = new FakeNode("span");
        label.className = "page-card__label";
        const id = new FakeNode("span");
        id.className = "page-card__id";
        meta.append(label, id);

        const stage = new FakeNode("div");
        stage.className = "page-stage";
        stage.hidden = true;
        const image = new FakeNode("img");
        image.className = "page-image";
        const tiles = new FakeNode("div");
        tiles.className = "page-tiles";
        const selected = new FakeNode("div");
        selected.className = "page-sync-marker page-sync-marker--selected";
        selected.hidden = true;
        const hovered = new FakeNode("div");
        hovered.className = "page-sync-marker page-sync-marker--hover";
        hovered.hidden = true;
        stage.append(image, tiles, selected, hovered);

        const frame = new FakeNode("iframe");
        frame.className = "page-frame";
        this.append(meta, stage, frame);
        return;
      }
      if (html.includes("source-line__number")) {
        const number = new FakeNode("span");
        number.className = "source-line__number";
        const text = new FakeNode("span");
        text.className = "source-line__text";
        this.append(number, text);
      }
    }

    get innerHTML() {
      return this._innerHTML;
    }
  }

  class FakeCustomEvent extends Event {
    constructor(type, init = {}) {
      super(type, init);
      this.detail = init.detail;
    }
  }

  const elementIds = [
    "revision",
    "build-status",
    "changed-files",
    "diagnostics",
    "source-status",
    "source-file",
    "source-selection",
    "source-viewer",
    "source-open",
    "source-link",
    "page-label",
    "zoom-label",
    "frame",
    "placeholder",
    "preview",
    "preview-stack",
    "prev-page",
    "next-page",
    "zoom-out",
    "zoom-in"
  ];
  const elements = new Map(elementIds.map((id) => [id, new FakeNode("div")]));
  const frame = elements.get("frame");
  frame.scrollTop = 0;
  const fakeDocument = {
    getElementById(id) {
      return elements.get(id) ?? null;
    },
    createElement(tagName) {
      return new FakeNode(tagName);
    },
    createDocumentFragment() {
      return new FakeNode("#fragment");
    }
  };

  const fetchCalls = [];
  const sockets = [];
  const fakeWindow = new EventTarget();
  fakeWindow.location = new URL("http://example.test/");
  if (typeof options.initialHash === "string" && options.initialHash.length > 0) {
    fakeWindow.location.hash = options.initialHash.startsWith("#")
      ? options.initialHash
      : `#${options.initialHash}`;
  }
  if (typeof options.initialHash === "string" && options.initialHash.length > 0) {
    fakeWindow.location.hash = options.initialHash;
  }
  fakeWindow.history = {
    replaceState(_state, _title, nextUrl) {
      fakeWindow.location = new URL(String(nextUrl), fakeWindow.location.href);
    }
  };
  fakeWindow.requestAnimationFrame = () => 1;
  fakeWindow.cancelAnimationFrame = () => {};

  const jsonResponse = (body, status = 200) => ({
    ok: status >= 200 && status < 300,
    status,
    async json() {
      return JSON.parse(JSON.stringify(body));
    }
  });

  const jumpPayload = {
    rev: 15,
    file: "main.tex",
    absolute_file: "/tmp/project/main.tex",
    file_uri: "file:///tmp/project/main.tex",
    editor_uri: "",
    editor_preview_kind: "none",
    offset_utf8: 8,
    line: 2,
    line0: 1,
    column: 4,
    column0: 3,
    source_hash: "#src=main.tex&line=2&column=4",
    editor_cwd: "/tmp/project",
    editor_launch_supported: false,
    editor_program: "",
    editor_args: [],
    editor_command_line: "",
    page_id: "page-a",
    page_index: 0,
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
  };

  const fakeFetch = async (input, init = {}) => {
    const url = new URL(typeof input === "string" ? input : String(input), fakeWindow.location.href);
    fetchCalls.push({
      url: url.toString(),
      method: init.method ?? "GET",
      body: init.body ?? null
    });
    if (url.pathname === "/api/state") {
      return jsonResponse({
        current_rev: 15,
        last_applied_rev: 15,
        pdf_url: "/artifacts/rev/15/main.pdf",
        page_ids: ["page-a"],
        page_artifacts: [
          {
            page_id: "page-a",
            pdf_url: "/artifacts/rev/15/pages/page-a.pdf",
            svg_url: "/artifacts/rev/15/pages/page-a.svg"
          }
        ],
        diagnostics: [],
        changed_files: [],
        building: false,
        last_build_succeeded: true,
        editor_bridge_enabled: false,
        ...(options.stateResponse ?? {})
      });
    }
    if (url.pathname === "/api/source-file/15") {
      return jsonResponse({
        rev: 15,
        file: "main.tex",
        content: "lead\nbody\ntail\n",
        line_count: 3
      });
    }
    if (url.pathname === "/api/source-jump/15") {
      if (typeof options.sourceJumpStatus === "number") {
        return jsonResponse({ error: "source jump failed" }, options.sourceJumpStatus);
      }
      return jsonResponse({
        ...jumpPayload,
        ...(options.sourceJumpResponse ?? {})
      });
    }
    if (url.pathname === "/api/open-source/15") {
      if (typeof options.openSourceStatus === "number") {
        return jsonResponse({ error: "open source failed" }, options.openSourceStatus);
      }
      return jsonResponse({
        ...jumpPayload,
        launched: false
      });
    }
    throw new Error(`unexpected fetch: ${url}`);
  };

  class FakeWebSocket extends EventTarget {
    static OPEN = 1;

    constructor() {
      super();
      this.readyState = 0;
      sockets.push(this);
    }

    send() {}
  }

  globalThis.window = fakeWindow;
  globalThis.document = fakeDocument;
  globalThis.fetch = fakeFetch;
  globalThis.WebSocket = FakeWebSocket;
  globalThis.CustomEvent = FakeCustomEvent;

  const flush = async () => {
    await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await Promise.resolve();
  };

  try {
    await import(moduleUrl);
    await flush();
    return await run({ window: fakeWindow, fetchCalls, flush, sockets });
  } finally {
    globalThis.window = previousWindow;
    globalThis.document = previousDocument;
    globalThis.fetch = previousFetch;
    globalThis.WebSocket = previousWebSocket;
    globalThis.CustomEvent = previousCustomEvent;
  }
}

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
    type: "ui_source_file_ready",
    rev: 9,
    file: "main.tex",
    content: "lead\nbody\n",
    line_count: 2
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

test("viewer open-source without editor bridge still fetches resolved payload", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush }) => {
    const resolvedEvents = [];
    window.addEventListener("latexd:open-source-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    await window.latexdSelectSource({ file: "main.tex", line: 2, column: 4 });
    await flush();

    const detail = await window.latexdOpenSelectedSource();
    await flush();

    const openSourceCall = fetchCalls.find((call) => call.url === "http://example.test/api/open-source/15");
    assert.ok(openSourceCall);
    assert.equal(openSourceCall.method, "POST");
    assert.deepEqual(JSON.parse(openSourceCall.body), {
      file: "main.tex",
      offset: null,
      line: 2,
      column: 4,
      source_hash: "#src=main.tex&line=2&column=4"
    });
    assert.equal(detail.launchRequested, true);
    assert.equal(detail.previewOnly, false);
    assert.equal(detail.launched, false);
    assert.equal(detail.absoluteFile, "/tmp/project/main.tex");
    assert.equal(detail.fileUri, "file:///tmp/project/main.tex");
    assert.equal(detail.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.item?.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.editorPreviewKind, "none");
    assert.equal(detail.editorLaunchSupported, false);
    assert.equal(resolvedEvents.length, 1);
    assert.equal(resolvedEvents[0].launchRequested, true);
    assert.equal(resolvedEvents[0].launched, false);
  });
});

test("viewer open-source accepts canonical source hash direct input", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush }) => {
    const detail = await window.latexdOpenSelectedSource("#src=main.tex&line=2&column=4");
    await flush();

    const openSourceCall = fetchCalls.find((call) => call.url === "http://example.test/api/open-source/15");
    assert.ok(openSourceCall);
    assert.equal(openSourceCall.method, "POST");
    assert.deepEqual(JSON.parse(openSourceCall.body), {
      source_hash: "#src=main.tex&line=2&column=4"
    });
    assert.deepEqual(detail.source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    assert.equal(detail.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.line, 2);
    assert.equal(detail.column, 4);
  });
});

test("viewer preview selected source without editor bridge keeps preview flags", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush }) => {
    const resolvedEvents = [];
    window.addEventListener("latexd:open-source-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    await window.latexdSelectSource({ file: "main.tex", line: 2, column: 4 });
    await flush();

    const detail = await window.latexdPreviewSelectedSource();
    await flush();

    const openSourceCall = fetchCalls.find((call) => call.url === "http://example.test/api/open-source/15");
    assert.ok(openSourceCall);
    assert.equal(openSourceCall.method, "POST");
    assert.deepEqual(JSON.parse(openSourceCall.body), {
      file: "main.tex",
      offset: null,
      line: 2,
      column: 4,
      source_hash: "#src=main.tex&line=2&column=4",
      launch: false
    });
    assert.equal(detail.launchRequested, false);
    assert.equal(detail.previewOnly, true);
    assert.equal(detail.launched, false);
    assert.equal(detail.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.editorPreviewKind, "none");
    assert.equal(detail.editorLaunchSupported, false);
    assert.equal(resolvedEvents.length, 1);
    assert.equal(resolvedEvents[0].previewOnly, true);
    assert.equal(resolvedEvents[0].launched, false);
  });
});

test("viewer open-source accepts canonical source hash input without prior selection", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush }) => {
    const resolvedEvents = [];
    window.addEventListener("latexd:open-source-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    const detail = await window.latexdOpenSelectedSource("#src=main.tex&line=2&column=4");
    await flush();

    const openSourceCall = fetchCalls.find((call) => call.url === "http://example.test/api/open-source/15");
    assert.ok(openSourceCall);
    assert.deepEqual(JSON.parse(openSourceCall.body), {
      source_hash: "#src=main.tex&line=2&column=4"
    });
    assert.deepEqual(detail.source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    assert.equal(detail.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.launchRequested, true);
    assert.equal(detail.previewOnly, false);
    assert.equal(detail.launched, false);
    assert.equal(resolvedEvents.length, 1);
    assert.equal(resolvedEvents[0].sourceHash, "#src=main.tex&line=2&column=4");
  });
});

test("viewer open-source direct object input prefers source hash over inconsistent explicit fields", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush }) => {
    const detail = await window.latexdOpenSelectedSource({
      file: "wrong.tex",
      line: 9,
      column: 1,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    await flush();

    const openSourceCall = fetchCalls.find((call) => call.url === "http://example.test/api/open-source/15");
    assert.ok(openSourceCall);
    assert.deepEqual(JSON.parse(openSourceCall.body), {
      source_hash: "#src=main.tex&line=2&column=4"
    });
    assert.deepEqual(detail.source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    assert.equal(detail.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.line, 2);
    assert.equal(detail.column, 4);
  });
});

test("viewer open-source accepts combined direct input object with launch false", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush }) => {
    const resolvedEvents = [];
    window.addEventListener("latexd:open-source-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    const detail = await window.latexdOpenSelectedSource({
      sourceHash: "#src=main.tex&line=2&column=4",
      launch: false
    });
    await flush();

    const openSourceCall = fetchCalls.find((call) => call.url === "http://example.test/api/open-source/15");
    assert.ok(openSourceCall);
    assert.deepEqual(JSON.parse(openSourceCall.body), {
      source_hash: "#src=main.tex&line=2&column=4",
      launch: false
    });
    assert.deepEqual(detail.source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    assert.equal(detail.launchRequested, false);
    assert.equal(detail.previewOnly, true);
    assert.equal(detail.launched, false);
    assert.equal(resolvedEvents.length, 1);
    assert.equal(resolvedEvents[0].previewOnly, true);
  });
});

test("viewer open-source accepts direct canonical input plus launch false options", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush }) => {
    const resolvedEvents = [];
    window.addEventListener("latexd:open-source-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    const detail = await window.latexdOpenSelectedSource(
      "#src=main.tex&line=2&column=4",
      { launch: false }
    );
    await flush();

    const openSourceCall = fetchCalls.find((call) => call.url === "http://example.test/api/open-source/15");
    assert.ok(openSourceCall);
    assert.deepEqual(JSON.parse(openSourceCall.body), {
      source_hash: "#src=main.tex&line=2&column=4",
      launch: false
    });
    assert.deepEqual(detail.source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    assert.equal(detail.launchRequested, false);
    assert.equal(detail.previewOnly, true);
    assert.equal(detail.launched, false);
    assert.equal(resolvedEvents.length, 1);
    assert.equal(resolvedEvents[0].previewOnly, true);
  });
});

test("viewer open-source legacy launch-only object still previews selected source", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush }) => {
    const resolvedEvents = [];
    window.addEventListener("latexd:open-source-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    await window.latexdSelectSource({ file: "main.tex", line: 2, column: 4 });
    await flush();

    const detail = await window.latexdOpenSelectedSource({ launch: false });
    await flush();

    const openSourceCall = fetchCalls.find((call) => call.url === "http://example.test/api/open-source/15");
    assert.ok(openSourceCall);
    assert.deepEqual(JSON.parse(openSourceCall.body), {
      file: "main.tex",
      offset: null,
      line: 2,
      column: 4,
      source_hash: "#src=main.tex&line=2&column=4",
      launch: false
    });
    assert.equal(detail.source?.file, "main.tex");
    assert.equal(detail.source?.startLine, 2);
    assert.equal(detail.source?.pageId, "page-a");
    assert.equal(detail.source?.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.launchRequested, false);
    assert.equal(detail.previewOnly, true);
    assert.equal(detail.launched, false);
    assert.equal(resolvedEvents.length, 1);
    assert.equal(resolvedEvents[0].previewOnly, true);
  });
});

test("viewer open-source explicit options override combined launch flag", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush }) => {
    const resolvedEvents = [];
    window.addEventListener("latexd:open-source-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    const detail = await window.latexdOpenSelectedSource(
      {
        sourceHash: "#src=main.tex&line=2&column=4",
        launch: true
      },
      { launch: false }
    );
    await flush();

    const openSourceCall = fetchCalls.find((call) => call.url === "http://example.test/api/open-source/15");
    assert.ok(openSourceCall);
    assert.deepEqual(JSON.parse(openSourceCall.body), {
      source_hash: "#src=main.tex&line=2&column=4",
      launch: false
    });
    assert.deepEqual(detail.source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    assert.equal(detail.launchRequested, false);
    assert.equal(detail.previewOnly, true);
    assert.equal(detail.launched, false);
    assert.equal(resolvedEvents.length, 1);
    assert.equal(resolvedEvents[0].previewOnly, true);
  });
});

test("viewer preview legacy launch-only object still previews selected source", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush }) => {
    const resolvedEvents = [];
    window.addEventListener("latexd:open-source-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    await window.latexdSelectSource({ file: "main.tex", line: 2, column: 4 });
    await flush();

    const detail = await window.latexdPreviewSelectedSource({ launch: true });
    await flush();

    const openSourceCall = fetchCalls.find((call) => call.url === "http://example.test/api/open-source/15");
    assert.ok(openSourceCall);
    assert.deepEqual(JSON.parse(openSourceCall.body), {
      file: "main.tex",
      offset: null,
      line: 2,
      column: 4,
      source_hash: "#src=main.tex&line=2&column=4",
      launch: false
    });
    assert.equal(detail.source?.file, "main.tex");
    assert.equal(detail.source?.startLine, 2);
    assert.equal(detail.source?.pageId, "page-a");
    assert.equal(detail.source?.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.launchRequested, false);
    assert.equal(detail.previewOnly, true);
    assert.equal(detail.launched, false);
    assert.equal(resolvedEvents.length, 1);
    assert.equal(resolvedEvents[0].previewOnly, true);
  });
});

test("viewer preview ignores combined launch flag on direct input object", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush }) => {
    const resolvedEvents = [];
    window.addEventListener("latexd:open-source-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    const detail = await window.latexdPreviewSelectedSource({
      sourceHash: "#src=main.tex&line=2&column=4",
      launch: true
    });
    await flush();

    const openSourceCall = fetchCalls.find((call) => call.url === "http://example.test/api/open-source/15");
    assert.ok(openSourceCall);
    assert.deepEqual(JSON.parse(openSourceCall.body), {
      source_hash: "#src=main.tex&line=2&column=4",
      launch: false
    });
    assert.deepEqual(detail.source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    assert.equal(detail.launchRequested, false);
    assert.equal(detail.previewOnly, true);
    assert.equal(detail.launched, false);
    assert.equal(resolvedEvents.length, 1);
    assert.equal(resolvedEvents[0].previewOnly, true);
  });
});

test("viewer preview accepts source-hash-only input object without prior selection", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush }) => {
    const resolvedEvents = [];
    window.addEventListener("latexd:open-source-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    const detail = await window.latexdPreviewSelectedSource({
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    await flush();

    const openSourceCall = fetchCalls.find((call) => call.url === "http://example.test/api/open-source/15");
    assert.ok(openSourceCall);
    assert.deepEqual(JSON.parse(openSourceCall.body), {
      source_hash: "#src=main.tex&line=2&column=4",
      launch: false
    });
    assert.deepEqual(detail.source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    assert.equal(detail.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.launchRequested, false);
    assert.equal(detail.previewOnly, true);
    assert.equal(detail.launched, false);
    assert.equal(resolvedEvents.length, 1);
    assert.equal(resolvedEvents[0].previewOnly, true);
  });
});

test("viewer preview launch-only object still falls back to selected source", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush }) => {
    const resolvedEvents = [];
    window.addEventListener("latexd:open-source-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    await window.latexdSelectSource({ file: "main.tex", line: 2, column: 4 });
    await flush();

    const detail = await window.latexdPreviewSelectedSource({ launch: true });
    await flush();

    const openSourceCall = fetchCalls.find((call) => call.url === "http://example.test/api/open-source/15");
    assert.ok(openSourceCall);
    assert.deepEqual(JSON.parse(openSourceCall.body), {
      file: "main.tex",
      offset: null,
      line: 2,
      column: 4,
      source_hash: "#src=main.tex&line=2&column=4",
      launch: false
    });
    assert.equal(detail.source?.file, "main.tex");
    assert.equal(detail.source?.startLine, 2);
    assert.equal(detail.source?.pageId, "page-a");
    assert.equal(detail.source?.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.launchRequested, false);
    assert.equal(detail.previewOnly, true);
    assert.equal(detail.launched, false);
    assert.equal(resolvedEvents.length, 1);
    assert.equal(resolvedEvents[0].previewOnly, true);
  });
});

test("viewer preview ignores conflicting launch flag on combined direct input", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush }) => {
    const resolvedEvents = [];
    window.addEventListener("latexd:open-source-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    const detail = await window.latexdPreviewSelectedSource({
      sourceHash: "#src=main.tex&line=2&column=4",
      launch: true
    });
    await flush();

    const openSourceCall = fetchCalls.find((call) => call.url === "http://example.test/api/open-source/15");
    assert.ok(openSourceCall);
    assert.deepEqual(JSON.parse(openSourceCall.body), {
      source_hash: "#src=main.tex&line=2&column=4",
      launch: false
    });
    assert.deepEqual(detail.source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    assert.equal(detail.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.launchRequested, false);
    assert.equal(detail.previewOnly, true);
    assert.equal(detail.launched, false);
    assert.equal(resolvedEvents.length, 1);
    assert.equal(resolvedEvents[0].previewOnly, true);
  });
});

test("viewer open-source combined preview input without applied revision returns local detail without fetching", async () => {
  await withViewerBrowserHarness({
    stateResponse: {
      current_rev: 0,
      last_applied_rev: 0,
      pdf_url: "",
      page_ids: [],
      page_artifacts: []
    }
  }, async ({ window, fetchCalls, flush }) => {
    const openEvents = [];
    const resolvedEvents = [];
    const failedEvents = [];
    window.addEventListener("latexd:open-source", (event) => {
      openEvents.push(event.detail);
    });
    window.addEventListener("latexd:open-source-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });
    window.addEventListener("latexd:open-source-failed", (event) => {
      failedEvents.push(event.detail);
    });

    const detail = await window.latexdOpenSelectedSource({
      sourceHash: "#src=main.tex&line=2&column=4",
      launch: false
    });
    await flush();

    const nonStateCalls = fetchCalls.filter((call) => new URL(call.url).pathname !== "/api/state");
    assert.deepEqual(nonStateCalls, []);
    assert.deepEqual(detail.source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    assert.deepEqual(detail.request, {
      source_hash: "#src=main.tex&line=2&column=4",
      launch: false
    });
    assert.equal(detail.rev, 0);
    assert.equal(detail.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.launchRequested, false);
    assert.equal(detail.previewOnly, true);
    assert.equal(openEvents.length, 1);
    assert.deepEqual(openEvents[0], detail);
    assert.equal(resolvedEvents.length, 0);
    assert.equal(failedEvents.length, 0);
  });
});

test("viewer preview conflicting launch input without applied revision returns preview detail without fetching", async () => {
  await withViewerBrowserHarness({
    stateResponse: {
      current_rev: 0,
      last_applied_rev: 0,
      pdf_url: "",
      page_ids: [],
      page_artifacts: []
    }
  }, async ({ window, fetchCalls, flush }) => {
    const openEvents = [];
    const resolvedEvents = [];
    const failedEvents = [];
    window.addEventListener("latexd:open-source", (event) => {
      openEvents.push(event.detail);
    });
    window.addEventListener("latexd:open-source-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });
    window.addEventListener("latexd:open-source-failed", (event) => {
      failedEvents.push(event.detail);
    });

    const detail = await window.latexdPreviewSelectedSource({
      sourceHash: "#src=main.tex&line=2&column=4",
      launch: true
    });
    await flush();

    const nonStateCalls = fetchCalls.filter((call) => new URL(call.url).pathname !== "/api/state");
    assert.deepEqual(nonStateCalls, []);
    assert.deepEqual(detail.source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    assert.deepEqual(detail.request, {
      source_hash: "#src=main.tex&line=2&column=4",
      launch: false
    });
    assert.equal(detail.rev, 0);
    assert.equal(detail.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.launchRequested, false);
    assert.equal(detail.previewOnly, true);
    assert.equal(openEvents.length, 1);
    assert.deepEqual(openEvents[0], detail);
    assert.equal(resolvedEvents.length, 0);
    assert.equal(failedEvents.length, 0);
  });
});

test("viewer jump and hover canonical object input without applied revision return null without fetching", async () => {
  await withViewerBrowserHarness({
    stateResponse: {
      current_rev: 0,
      last_applied_rev: 0,
      pdf_url: "",
      page_ids: [],
      page_artifacts: []
    }
  }, async ({ window, fetchCalls, flush }) => {
    const jumpResolvedEvents = [];
    const jumpFailedEvents = [];
    const hoverResolvedEvents = [];
    const hoverFailedEvents = [];
    window.addEventListener("latexd:source-jump-resolved", (event) => {
      jumpResolvedEvents.push(event.detail);
    });
    window.addEventListener("latexd:source-jump-failed", (event) => {
      jumpFailedEvents.push(event.detail);
    });
    window.addEventListener("latexd:source-hover-resolved", (event) => {
      hoverResolvedEvents.push(event.detail);
    });
    window.addEventListener("latexd:source-hover-failed", (event) => {
      hoverFailedEvents.push(event.detail);
    });

    const jumpDetail = await window.latexdJumpToSource({
      file: "wrong.tex",
      line: 9,
      column: 1,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    const hoverDetail = await window.latexdHoverSource({
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    await flush();

    const nonStateCalls = fetchCalls.filter((call) => new URL(call.url).pathname !== "/api/state");
    assert.deepEqual(nonStateCalls, []);
    assert.equal(jumpDetail, null);
    assert.equal(hoverDetail, null);
    assert.equal(jumpResolvedEvents.length, 0);
    assert.equal(jumpFailedEvents.length, 0);
    assert.equal(hoverResolvedEvents.length, 0);
    assert.equal(hoverFailedEvents.length, 0);
  });
});

test("viewer open-source and preview without request stay silent without an applied revision", async () => {
  await withViewerBrowserHarness({
    stateResponse: {
      current_rev: 0,
      last_applied_rev: 0,
      pdf_url: "",
      page_ids: [],
      page_artifacts: []
    }
  }, async ({ window, fetchCalls, flush }) => {
    const intentEvents = [];
    const resolvedEvents = [];
    const failedEvents = [];
    window.addEventListener("latexd:open-source", (event) => {
      intentEvents.push(event.detail);
    });
    window.addEventListener("latexd:open-source-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });
    window.addEventListener("latexd:open-source-failed", (event) => {
      failedEvents.push(event.detail);
    });

    const openDetail = await window.latexdOpenSelectedSource();
    const previewDetail = await window.latexdPreviewSelectedSource({});
    await flush();

    assert.equal(fetchCalls.length, 1);
    assert.equal(fetchCalls[0].url, "http://example.test/api/state");
    assert.equal(openDetail, null);
    assert.equal(previewDetail, null);
    assert.equal(intentEvents.length, 0);
    assert.equal(resolvedEvents.length, 0);
    assert.equal(failedEvents.length, 0);
  });
});

test("viewer open-source and preview without request stay silent with an applied revision", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush }) => {
    const intentEvents = [];
    const resolvedEvents = [];
    const failedEvents = [];
    window.addEventListener("latexd:open-source", (event) => {
      intentEvents.push(event.detail);
    });
    window.addEventListener("latexd:open-source-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });
    window.addEventListener("latexd:open-source-failed", (event) => {
      failedEvents.push(event.detail);
    });

    const openDetail = await window.latexdOpenSelectedSource();
    const openLaunchOnlyDetail = await window.latexdOpenSelectedSource({ launch: false });
    const previewDetail = await window.latexdPreviewSelectedSource({});
    const previewNullDetail = await window.latexdPreviewSelectedSource(null);
    await flush();

    assert.equal(fetchCalls.length, 1);
    assert.equal(fetchCalls[0].url, "http://example.test/api/state");
    assert.equal(openDetail, null);
    assert.equal(openLaunchOnlyDetail, null);
    assert.equal(previewDetail, null);
    assert.equal(previewNullDetail, null);
    assert.equal(intentEvents.length, 0);
    assert.equal(resolvedEvents.length, 0);
    assert.equal(failedEvents.length, 0);
  });
});

test("viewer jump and hover without request stay silent without an applied revision", async () => {
  await withViewerBrowserHarness({
    stateResponse: {
      current_rev: 0,
      last_applied_rev: 0,
      pdf_url: "",
      page_ids: [],
      page_artifacts: []
    }
  }, async ({ window, fetchCalls, flush }) => {
    const jumpResolvedEvents = [];
    const jumpFailedEvents = [];
    const hoverResolvedEvents = [];
    const hoverFailedEvents = [];
    window.addEventListener("latexd:source-jump-resolved", (event) => {
      jumpResolvedEvents.push(event.detail);
    });
    window.addEventListener("latexd:source-jump-failed", (event) => {
      jumpFailedEvents.push(event.detail);
    });
    window.addEventListener("latexd:source-hover-resolved", (event) => {
      hoverResolvedEvents.push(event.detail);
    });
    window.addEventListener("latexd:source-hover-failed", (event) => {
      hoverFailedEvents.push(event.detail);
    });

    const jumpDetail = await window.latexdJumpToSource({});
    const hoverDetail = await window.latexdHoverSource();
    await flush();

    assert.equal(fetchCalls.length, 1);
    assert.equal(fetchCalls[0].url, "http://example.test/api/state");
    assert.equal(jumpDetail, null);
    assert.equal(hoverDetail, null);
    assert.equal(jumpResolvedEvents.length, 0);
    assert.equal(jumpFailedEvents.length, 0);
    assert.equal(hoverResolvedEvents.length, 0);
    assert.equal(hoverFailedEvents.length, 0);
  });
});

test("viewer jump and hover without request stay silent with an applied revision", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush }) => {
    const jumpResolvedEvents = [];
    const jumpFailedEvents = [];
    const hoverResolvedEvents = [];
    const hoverFailedEvents = [];
    window.addEventListener("latexd:source-jump-resolved", (event) => {
      jumpResolvedEvents.push(event.detail);
    });
    window.addEventListener("latexd:source-jump-failed", (event) => {
      jumpFailedEvents.push(event.detail);
    });
    window.addEventListener("latexd:source-hover-resolved", (event) => {
      hoverResolvedEvents.push(event.detail);
    });
    window.addEventListener("latexd:source-hover-failed", (event) => {
      hoverFailedEvents.push(event.detail);
    });

    const jumpNoArgDetail = await window.latexdJumpToSource();
    const jumpDetail = await window.latexdJumpToSource({});
    const hoverEmptyObjectDetail = await window.latexdHoverSource({});
    const hoverDetail = await window.latexdHoverSource(null);
    await flush();

    assert.equal(fetchCalls.length, 1);
    assert.equal(fetchCalls[0].url, "http://example.test/api/state");
    assert.equal(jumpNoArgDetail, null);
    assert.equal(jumpDetail, null);
    assert.equal(hoverEmptyObjectDetail, null);
    assert.equal(hoverDetail, null);
    assert.equal(jumpResolvedEvents.length, 0);
    assert.equal(jumpFailedEvents.length, 0);
    assert.equal(hoverResolvedEvents.length, 0);
    assert.equal(hoverFailedEvents.length, 0);
  });
});

test("viewer open-source and preview stay silent after full preview refresh clears selected source", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush, sockets }) => {
    assert.equal(sockets.length, 1);

    const selectedEvents = [];
    window.addEventListener("latexd:source-selected", (event) => {
      selectedEvents.push(event.detail);
    });

    await window.latexdSelectSource({ file: "main.tex", line: 2, column: 4 });
    await flush();

    sockets[0].dispatchEvent(new MessageEvent("message", {
      data: JSON.stringify({
        type: "full_pdf_ready",
        rev: 16,
        pdf_url: "/artifacts/rev/16/main.pdf",
        page_ids: ["page-a"],
        page_artifacts: [
          {
            page_id: "page-a",
            pdf_url: "/artifacts/rev/16/pages/page-a.pdf",
            svg_url: "/artifacts/rev/16/pages/page-a.svg"
          }
        ]
      })
    }));
    await flush();

    assert.deepEqual(selectedEvents.at(-1), {
      rev: 16,
      source: null
    });

    const intentEvents = [];
    const resolvedEvents = [];
    const failedEvents = [];
    window.addEventListener("latexd:open-source", (event) => {
      intentEvents.push(event.detail);
    });
    window.addEventListener("latexd:open-source-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });
    window.addEventListener("latexd:open-source-failed", (event) => {
      failedEvents.push(event.detail);
    });
    const callCountBefore = fetchCalls.length;

    const openDetail = await window.latexdOpenSelectedSource();
    const previewDetail = await window.latexdPreviewSelectedSource();
    await flush();

    assert.equal(fetchCalls.length, callCountBefore);
    assert.equal(openDetail, null);
    assert.equal(previewDetail, null);
    assert.equal(intentEvents.length, 0);
    assert.equal(resolvedEvents.length, 0);
    assert.equal(failedEvents.length, 0);
  });
});

test("viewer open-source and preview stay silent after patch-pages clears hovered source", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush, sockets }) => {
    assert.equal(sockets.length, 1);

    const hoveredEvents = [];
    window.addEventListener("latexd:source-hovered", (event) => {
      hoveredEvents.push(event.detail);
    });

    await window.latexdHoverSource({ file: "main.tex", line: 2, column: 4 });
    await flush();

    sockets[0].dispatchEvent(new MessageEvent("message", {
      data: JSON.stringify({
        type: "patch_pages",
        rev: 16,
        ops: [
          {
            op: "replace_page",
            index: 0,
            page_id: "page-a",
            pdf_url: "/artifacts/rev/16/pages/page-a.pdf",
            svg_url: "/artifacts/rev/16/pages/page-a.svg"
          }
        ]
      })
    }));
    await flush();

    assert.deepEqual(hoveredEvents.at(-1), {
      rev: 15,
      source: null
    });

    const intentEvents = [];
    const resolvedEvents = [];
    const failedEvents = [];
    window.addEventListener("latexd:open-source", (event) => {
      intentEvents.push(event.detail);
    });
    window.addEventListener("latexd:open-source-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });
    window.addEventListener("latexd:open-source-failed", (event) => {
      failedEvents.push(event.detail);
    });
    const callCountBefore = fetchCalls.length;

    const openDetail = await window.latexdOpenSelectedSource();
    const previewDetail = await window.latexdPreviewSelectedSource();
    await flush();

    assert.equal(fetchCalls.length, callCountBefore);
    assert.equal(openDetail, null);
    assert.equal(previewDetail, null);
    assert.equal(intentEvents.length, 0);
    assert.equal(resolvedEvents.length, 0);
    assert.equal(failedEvents.length, 0);
  });
});

test("viewer open-source and preview stay silent after syncmap refresh clears selected source", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush, sockets }) => {
    assert.equal(sockets.length, 1);

    const selectedEvents = [];
    window.addEventListener("latexd:source-selected", (event) => {
      selectedEvents.push(event.detail);
    });

    await window.latexdSelectSource({ file: "main.tex", line: 2, column: 4 });
    await flush();

    sockets[0].dispatchEvent(new MessageEvent("message", {
      data: JSON.stringify({
        type: "ui_syncmap_ready",
        rev: 15,
        page_id: "page-a",
        page_width_px: 612,
        page_height_px: 792,
        page_source_start_utf8: 0,
        page_source_end_utf8: 5,
        page_output_start_utf8: 0,
        page_output_end_utf8: 24,
        items: [
          {
            file: "main.tex",
            start_utf8: 0,
            end_utf8: 5,
            output_start_utf8: 0,
            output_end_utf8: 24,
            start_line: 1,
            end_line: 1,
            left_px: 72,
            right_px: 144,
            top_px: 0,
            bottom_px: 100
          }
        ]
      })
    }));
    await flush();

    assert.deepEqual(selectedEvents.at(-1), {
      rev: 15,
      source: null
    });

    const intentEvents = [];
    const resolvedEvents = [];
    const failedEvents = [];
    window.addEventListener("latexd:open-source", (event) => {
      intentEvents.push(event.detail);
    });
    window.addEventListener("latexd:open-source-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });
    window.addEventListener("latexd:open-source-failed", (event) => {
      failedEvents.push(event.detail);
    });
    const callCountBefore = fetchCalls.length;

    const openDetail = await window.latexdOpenSelectedSource();
    const previewDetail = await window.latexdPreviewSelectedSource();
    await flush();

    assert.equal(fetchCalls.length, callCountBefore);
    assert.equal(openDetail, null);
    assert.equal(previewDetail, null);
    assert.equal(intentEvents.length, 0);
    assert.equal(resolvedEvents.length, 0);
    assert.equal(failedEvents.length, 0);
  });
});

test("viewer open-source and preview stay silent after syncmap refresh clears hovered source", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush, sockets }) => {
    assert.equal(sockets.length, 1);

    const hoveredEvents = [];
    window.addEventListener("latexd:source-hovered", (event) => {
      hoveredEvents.push(event.detail);
    });

    await window.latexdHoverSource({ file: "main.tex", line: 2, column: 4 });
    await flush();

    sockets[0].dispatchEvent(new MessageEvent("message", {
      data: JSON.stringify({
        type: "ui_syncmap_ready",
        rev: 15,
        page_id: "page-a",
        page_width_px: 612,
        page_height_px: 792,
        page_source_start_utf8: 0,
        page_source_end_utf8: 5,
        page_output_start_utf8: 0,
        page_output_end_utf8: 24,
        items: [
          {
            file: "main.tex",
            start_utf8: 0,
            end_utf8: 5,
            output_start_utf8: 0,
            output_end_utf8: 24,
            start_line: 1,
            end_line: 1,
            left_px: 72,
            right_px: 144,
            top_px: 0,
            bottom_px: 100
          }
        ]
      })
    }));
    await flush();

    assert.deepEqual(hoveredEvents.at(-1), {
      rev: 15,
      source: null
    });

    const intentEvents = [];
    const resolvedEvents = [];
    const failedEvents = [];
    window.addEventListener("latexd:open-source", (event) => {
      intentEvents.push(event.detail);
    });
    window.addEventListener("latexd:open-source-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });
    window.addEventListener("latexd:open-source-failed", (event) => {
      failedEvents.push(event.detail);
    });
    const callCountBefore = fetchCalls.length;

    const openDetail = await window.latexdOpenSelectedSource();
    const previewDetail = await window.latexdPreviewSelectedSource();
    await flush();

    assert.equal(fetchCalls.length, callCountBefore);
    assert.equal(openDetail, null);
    assert.equal(previewDetail, null);
    assert.equal(intentEvents.length, 0);
    assert.equal(resolvedEvents.length, 0);
    assert.equal(failedEvents.length, 0);
  });
});

test("viewer open-source failure emits explicit failed event", async () => {
  await withViewerBrowserHarness({ openSourceStatus: 500 }, async ({ window, fetchCalls, flush }) => {
    const failedEvents = [];
    const resolvedEvents = [];
    window.addEventListener("latexd:open-source-failed", (event) => {
      failedEvents.push(event.detail);
    });
    window.addEventListener("latexd:open-source-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    const detail = await window.latexdOpenSelectedSource("#src=main.tex&line=2&column=4");
    await flush();

    const openSourceCall = fetchCalls.find((call) => call.url === "http://example.test/api/open-source/15");
    assert.ok(openSourceCall);
    assert.deepEqual(detail.source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    assert.equal(detail.launchRequested, true);
    assert.equal(detail.previewOnly, false);
    assert.equal(detail.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.error, "open source request failed: 500");
    assert.equal(resolvedEvents.length, 0);
    assert.equal(failedEvents.length, 1);
    assert.deepEqual(failedEvents[0].source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    assert.equal(failedEvents[0].error, "open source request failed: 500");
    assert.equal(failedEvents[0].launchRequested, true);
    assert.equal(failedEvents[0].previewOnly, false);
  });
});

test("viewer preview failure preserves selected-source fallback detail", async () => {
  await withViewerBrowserHarness({ openSourceStatus: 500 }, async ({ window, fetchCalls, flush }) => {
    const failedEvents = [];
    const resolvedEvents = [];
    window.addEventListener("latexd:open-source-failed", (event) => {
      failedEvents.push(event.detail);
    });
    window.addEventListener("latexd:open-source-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    await window.latexdSelectSource({ file: "main.tex", line: 2, column: 4 });
    await flush();

    const detail = await window.latexdPreviewSelectedSource({ launch: true });
    await flush();

    const openSourceCall = fetchCalls.find((call) => call.url === "http://example.test/api/open-source/15");
    assert.ok(openSourceCall);
    assert.deepEqual(JSON.parse(openSourceCall.body), {
      file: "main.tex",
      offset: null,
      line: 2,
      column: 4,
      source_hash: "#src=main.tex&line=2&column=4",
      launch: false
    });
    assert.equal(detail.source?.file, "main.tex");
    assert.equal(detail.source?.startLine, 2);
    assert.equal(detail.source?.pageId, "page-a");
    assert.equal(detail.source?.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.launchRequested, false);
    assert.equal(detail.previewOnly, true);
    assert.equal(detail.error, "open source request failed: 500");
    assert.equal(resolvedEvents.length, 0);
    assert.equal(failedEvents.length, 1);
    assert.equal(failedEvents[0].previewOnly, true);
    assert.equal(failedEvents[0].source?.pageId, "page-a");
  });
});

test("viewer preview failure ignores conflicting launch flag on direct input", async () => {
  await withViewerBrowserHarness({ openSourceStatus: 500 }, async ({ window, fetchCalls, flush }) => {
    const failedEvents = [];
    const resolvedEvents = [];
    window.addEventListener("latexd:open-source-failed", (event) => {
      failedEvents.push(event.detail);
    });
    window.addEventListener("latexd:open-source-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    const detail = await window.latexdPreviewSelectedSource({
      sourceHash: "#src=main.tex&line=2&column=4",
      launch: true
    });
    await flush();

    const openSourceCall = fetchCalls.find((call) => call.url === "http://example.test/api/open-source/15");
    assert.ok(openSourceCall);
    assert.deepEqual(JSON.parse(openSourceCall.body), {
      source_hash: "#src=main.tex&line=2&column=4",
      launch: false
    });
    assert.deepEqual(detail.source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    assert.equal(detail.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.launchRequested, false);
    assert.equal(detail.previewOnly, true);
    assert.equal(detail.error, "open source request failed: 500");
    assert.equal(resolvedEvents.length, 0);
    assert.equal(failedEvents.length, 1);
    assert.equal(failedEvents[0].previewOnly, true);
    assert.deepEqual(failedEvents[0].source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
  });
});

test("viewer source jump failure emits explicit failed event", async () => {
  await withViewerBrowserHarness({ sourceJumpStatus: 500 }, async ({ window, fetchCalls, flush }) => {
    const failedEvents = [];
    const resolvedEvents = [];
    window.addEventListener("latexd:source-jump-failed", (event) => {
      failedEvents.push(event.detail);
    });
    window.addEventListener("latexd:source-jump-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    const result = await window.latexdJumpToSource({ file: "main.tex", line: 2, column: 4 });
    await flush();

    const jumpCall = fetchCalls.find((call) => call.url === "http://example.test/api/source-jump/15?file=main.tex&line=2&column=4");
    assert.ok(jumpCall);
    assert.equal(result.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(result.error, "source jump request failed: 500");
    assert.equal(resolvedEvents.length, 0);
    assert.equal(failedEvents.length, 1);
    assert.deepEqual(failedEvents[0].request, {
      file: "main.tex",
      offset: null,
      line: 2,
      column: 4
    });
    assert.equal(failedEvents[0].rev, 15);
    assert.equal(failedEvents[0].error, "source jump request failed: 500");
    assert.equal(failedEvents[0].sourceHash, "#src=main.tex&line=2&column=4");
  });
});

test("viewer source jump direct hash failure preserves canonical source detail", async () => {
  await withViewerBrowserHarness({ sourceJumpStatus: 500 }, async ({ window, fetchCalls, flush }) => {
    const failedEvents = [];
    window.addEventListener("latexd:source-jump-failed", (event) => {
      failedEvents.push(event.detail);
    });

    const result = await window.latexdJumpToSource("#src=main.tex&line=2&column=4");
    await flush();

    const jumpCall = fetchCalls.find((call) => call.url.startsWith("http://example.test/api/source-jump/15?"));
    assert.ok(jumpCall);
    const jumpUrl = new URL(jumpCall.url);
    assert.equal(jumpUrl.searchParams.get("source_hash"), "#src=main.tex&line=2&column=4");
    assert.deepEqual(result.source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    assert.equal(result.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(result.error, "source jump request failed: 500");
    assert.equal(failedEvents.length, 1);
    assert.deepEqual(failedEvents[0].source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    assert.equal(failedEvents[0].sourceHash, "#src=main.tex&line=2&column=4");
  });
});

test("viewer source jump direct object failure prefers source hash over inconsistent explicit fields", async () => {
  await withViewerBrowserHarness({ sourceJumpStatus: 500 }, async ({ window, fetchCalls, flush }) => {
    const failedEvents = [];
    window.addEventListener("latexd:source-jump-failed", (event) => {
      failedEvents.push(event.detail);
    });

    const result = await window.latexdJumpToSource({
      file: "wrong.tex",
      line: 9,
      column: 1,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    await flush();

    const jumpCall = fetchCalls.find((call) => call.url.startsWith("http://example.test/api/source-jump/15?"));
    assert.ok(jumpCall);
    const jumpUrl = new URL(jumpCall.url);
    assert.equal(jumpUrl.searchParams.get("source_hash"), "#src=main.tex&line=2&column=4");
    assert.equal(jumpUrl.searchParams.get("file"), "main.tex");
    assert.equal(jumpUrl.searchParams.get("line"), "2");
    assert.equal(jumpUrl.searchParams.get("column"), "4");
    assert.deepEqual(result.source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    assert.equal(result.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(result.error, "source jump request failed: 500");
    assert.equal(failedEvents.length, 1);
    assert.deepEqual(failedEvents[0].source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
  });
});

test("viewer hover source failure emits explicit failed event", async () => {
  await withViewerBrowserHarness({ sourceJumpStatus: 500 }, async ({ window, fetchCalls, flush }) => {
    const failedEvents = [];
    const resolvedEvents = [];
    window.addEventListener("latexd:source-hover-failed", (event) => {
      failedEvents.push(event.detail);
    });
    window.addEventListener("latexd:source-hover-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    const result = await window.latexdHoverSource({ file: "main.tex", line: 2, column: 4 });
    await flush();

    const jumpCall = fetchCalls.find((call) => call.url === "http://example.test/api/source-jump/15?file=main.tex&line=2&column=4");
    assert.ok(jumpCall);
    assert.equal(result.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(result.error, "source jump request failed: 500");
    assert.equal(resolvedEvents.length, 0);
    assert.equal(failedEvents.length, 1);
    assert.equal(failedEvents[0].error, "source jump request failed: 500");
    assert.equal(failedEvents[0].sourceHash, "#src=main.tex&line=2&column=4");
  });
});

test("viewer hover direct hash failure preserves canonical source detail", async () => {
  await withViewerBrowserHarness({ sourceJumpStatus: 500 }, async ({ window, fetchCalls, flush }) => {
    const failedEvents = [];
    window.addEventListener("latexd:source-hover-failed", (event) => {
      failedEvents.push(event.detail);
    });

    const result = await window.latexdHoverSource("#src=main.tex&line=2&column=4");
    await flush();

    const jumpCall = fetchCalls.find((call) => call.url.startsWith("http://example.test/api/source-jump/15?"));
    assert.ok(jumpCall);
    const jumpUrl = new URL(jumpCall.url);
    assert.equal(jumpUrl.searchParams.get("source_hash"), "#src=main.tex&line=2&column=4");
    assert.deepEqual(result.source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    assert.equal(result.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(result.error, "source jump request failed: 500");
    assert.equal(failedEvents.length, 1);
    assert.deepEqual(failedEvents[0].source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    assert.equal(failedEvents[0].sourceHash, "#src=main.tex&line=2&column=4");
  });
});

test("viewer hover direct object failure prefers source hash over inconsistent explicit fields", async () => {
  await withViewerBrowserHarness({ sourceJumpStatus: 500 }, async ({ window, fetchCalls, flush }) => {
    const failedEvents = [];
    window.addEventListener("latexd:source-hover-failed", (event) => {
      failedEvents.push(event.detail);
    });

    const result = await window.latexdHoverSource({
      file: "wrong.tex",
      line: 9,
      column: 1,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    await flush();

    const jumpCall = fetchCalls.find((call) => call.url.startsWith("http://example.test/api/source-jump/15?"));
    assert.ok(jumpCall);
    const jumpUrl = new URL(jumpCall.url);
    assert.equal(jumpUrl.searchParams.get("source_hash"), "#src=main.tex&line=2&column=4");
    assert.equal(jumpUrl.searchParams.get("file"), "main.tex");
    assert.equal(jumpUrl.searchParams.get("line"), "2");
    assert.equal(jumpUrl.searchParams.get("column"), "4");
    assert.deepEqual(result.source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    assert.equal(result.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(result.error, "source jump request failed: 500");
    assert.equal(failedEvents.length, 1);
    assert.deepEqual(failedEvents[0].source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    assert.equal(failedEvents[0].sourceHash, "#src=main.tex&line=2&column=4");
  });
});

test("viewer open-source without an applied revision returns immediate local detail", async () => {
  await withViewerBrowserHarness({
    stateResponse: {
      current_rev: 0,
      last_applied_rev: 0,
      pdf_url: "",
      page_ids: [],
      page_artifacts: []
    }
  }, async ({ window, fetchCalls, flush }) => {
    const intentEvents = [];
    const resolvedEvents = [];
    const failedEvents = [];
    window.addEventListener("latexd:open-source", (event) => {
      intentEvents.push(event.detail);
    });
    window.addEventListener("latexd:open-source-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });
    window.addEventListener("latexd:open-source-failed", (event) => {
      failedEvents.push(event.detail);
    });

    const detail = await window.latexdOpenSelectedSource("#src=main.tex&line=2&column=4");
    await flush();

    assert.equal(fetchCalls.length, 1);
    assert.equal(fetchCalls[0].url, "http://example.test/api/state");
    assert.equal(detail.rev, 0);
    assert.deepEqual(detail.request, {
      source_hash: "#src=main.tex&line=2&column=4"
    });
    assert.deepEqual(detail.source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    assert.equal(detail.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.launchRequested, true);
    assert.equal(detail.previewOnly, false);
    assert.equal(intentEvents.length, 1);
    assert.equal(intentEvents[0].rev, 0);
    assert.equal(resolvedEvents.length, 0);
    assert.equal(failedEvents.length, 0);
  });
});

test("viewer preview without an applied revision returns immediate local detail", async () => {
  await withViewerBrowserHarness({
    stateResponse: {
      current_rev: 0,
      last_applied_rev: 0,
      pdf_url: "",
      page_ids: [],
      page_artifacts: []
    }
  }, async ({ window, fetchCalls, flush }) => {
    const intentEvents = [];
    const resolvedEvents = [];
    const failedEvents = [];
    window.addEventListener("latexd:open-source", (event) => {
      intentEvents.push(event.detail);
    });
    window.addEventListener("latexd:open-source-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });
    window.addEventListener("latexd:open-source-failed", (event) => {
      failedEvents.push(event.detail);
    });

    const detail = await window.latexdPreviewSelectedSource("#src=main.tex&line=2&column=4");
    await flush();

    assert.equal(fetchCalls.length, 1);
    assert.equal(fetchCalls[0].url, "http://example.test/api/state");
    assert.equal(detail.rev, 0);
    assert.deepEqual(detail.request, {
      source_hash: "#src=main.tex&line=2&column=4",
      launch: false
    });
    assert.deepEqual(detail.source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    assert.equal(detail.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.launchRequested, false);
    assert.equal(detail.previewOnly, true);
    assert.equal(intentEvents.length, 1);
    assert.equal(intentEvents[0].previewOnly, true);
    assert.equal(resolvedEvents.length, 0);
    assert.equal(failedEvents.length, 0);
  });
});

test("viewer source jump without an applied revision returns null and makes no request", async () => {
  await withViewerBrowserHarness({
    stateResponse: {
      current_rev: 0,
      last_applied_rev: 0,
      pdf_url: "",
      page_ids: [],
      page_artifacts: []
    }
  }, async ({ window, fetchCalls, flush }) => {
    const resolvedEvents = [];
    const failedEvents = [];
    window.addEventListener("latexd:source-jump-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });
    window.addEventListener("latexd:source-jump-failed", (event) => {
      failedEvents.push(event.detail);
    });

    const detail = await window.latexdJumpToSource("#src=main.tex&line=2&column=4");
    await flush();

    assert.equal(fetchCalls.length, 1);
    assert.equal(fetchCalls[0].url, "http://example.test/api/state");
    assert.equal(detail, null);
    assert.equal(resolvedEvents.length, 0);
    assert.equal(failedEvents.length, 0);
  });
});

test("viewer source hover without an applied revision returns null and makes no request", async () => {
  await withViewerBrowserHarness({
    stateResponse: {
      current_rev: 0,
      last_applied_rev: 0,
      pdf_url: "",
      page_ids: [],
      page_artifacts: []
    }
  }, async ({ window, fetchCalls, flush }) => {
    const resolvedEvents = [];
    const failedEvents = [];
    window.addEventListener("latexd:source-hover-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });
    window.addEventListener("latexd:source-hover-failed", (event) => {
      failedEvents.push(event.detail);
    });

    const detail = await window.latexdHoverSource("#src=main.tex&line=2&column=4");
    await flush();

    assert.equal(fetchCalls.length, 1);
    assert.equal(fetchCalls[0].url, "http://example.test/api/state");
    assert.equal(detail, null);
    assert.equal(resolvedEvents.length, 0);
    assert.equal(failedEvents.length, 0);
  });
});

test("viewer startup source hash auto-jumps once after state load", async () => {
  await withViewerBrowserHarness({
    initialHash: "#src=main.tex&line=2&column=4"
  }, async ({ window, fetchCalls, flush }) => {
    const jumpCallsBeforeOpen = fetchCalls.filter((call) =>
      call.url.startsWith("http://example.test/api/source-jump/15?")
    );
    assert.equal(jumpCallsBeforeOpen.length, 1);
    const jumpUrl = new URL(jumpCallsBeforeOpen[0].url);
    assert.equal(jumpUrl.searchParams.get("source_hash"), null);
    assert.equal(jumpUrl.searchParams.get("file"), "main.tex");
    assert.equal(jumpUrl.searchParams.get("line"), "2");
    assert.equal(jumpUrl.searchParams.get("column"), "4");

    const detail = await window.latexdOpenSelectedSource();
    await flush();

    const jumpCallsAfterOpen = fetchCalls.filter((call) =>
      call.url.startsWith("http://example.test/api/source-jump/15?")
    );
    const openSourceCalls = fetchCalls.filter((call) =>
      call.url === "http://example.test/api/open-source/15"
    );
    assert.equal(jumpCallsAfterOpen.length, 1);
    assert.equal(openSourceCalls.length, 1);
    assert.equal(detail.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.item?.sourceHash, "#src=main.tex&line=2&column=4");
  });
});

test("viewer startup source hash waits for first applied revision before flushing", async () => {
  await withViewerBrowserHarness({
    initialHash: "#src=main.tex&line=2&column=4",
    stateResponse: {
      current_rev: 0,
      last_applied_rev: 0,
      pdf_url: "",
      page_ids: [],
      page_artifacts: []
    }
  }, async ({ window, fetchCalls, flush, sockets }) => {
    const resolvedEvents = [];
    window.addEventListener("latexd:source-jump-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    const initialJumpCalls = fetchCalls.filter((call) =>
      call.url.startsWith("http://example.test/api/source-jump/")
    );
    assert.equal(initialJumpCalls.length, 0);

    sockets[0].dispatchEvent(new MessageEvent("message", {
      data: JSON.stringify({
        type: "full_pdf_ready",
        rev: 15,
        pdf_url: "/artifacts/rev/15/main.pdf",
        page_ids: ["page-a"],
        page_artifacts: [
          {
            page_id: "page-a",
            pdf_url: "/artifacts/rev/15/pages/page-a.pdf",
            svg_url: "/artifacts/rev/15/pages/page-a.svg"
          }
        ]
      })
    }));
    await flush();

    const jumpCallsAfterRefresh = fetchCalls.filter((call) =>
      call.url.startsWith("http://example.test/api/source-jump/15?")
    );
    assert.equal(jumpCallsAfterRefresh.length, 1);
    const jumpUrl = new URL(jumpCallsAfterRefresh[0].url);
    assert.equal(jumpUrl.searchParams.get("source_hash"), null);
    assert.equal(jumpUrl.searchParams.get("file"), "main.tex");
    assert.equal(jumpUrl.searchParams.get("line"), "2");
    assert.equal(jumpUrl.searchParams.get("column"), "4");
    assert.equal(resolvedEvents.length, 1);
    assert.equal(resolvedEvents[0].sourceHash, "#src=main.tex&line=2&column=4");

    const detail = await window.latexdOpenSelectedSource();
    await flush();

    const openSourceCalls = fetchCalls.filter((call) =>
      call.url === "http://example.test/api/open-source/15"
    );
    assert.equal(openSourceCalls.length, 1);
    assert.equal(detail.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.item?.sourceHash, "#src=main.tex&line=2&column=4");
  });
});

test("viewer hashchange source hash auto-jumps immediately with an applied revision", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush }) => {
    const resolvedEvents = [];
    window.addEventListener("latexd:source-jump-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    window.location.hash = "#src=main.tex&line=2&column=4";
    window.dispatchEvent(new Event("hashchange"));
    await flush();

    const jumpCalls = fetchCalls.filter((call) =>
      call.url.startsWith("http://example.test/api/source-jump/15?")
    );
    assert.equal(jumpCalls.length, 1);
    const jumpUrl = new URL(jumpCalls[0].url);
    assert.equal(jumpUrl.searchParams.get("source_hash"), null);
    assert.equal(jumpUrl.searchParams.get("file"), "main.tex");
    assert.equal(jumpUrl.searchParams.get("line"), "2");
    assert.equal(jumpUrl.searchParams.get("column"), "4");
    assert.equal(resolvedEvents.length, 1);
    assert.equal(resolvedEvents[0].sourceHash, "#src=main.tex&line=2&column=4");
  });
});

test("viewer repeated identical hashchange does not duplicate source jump work", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush }) => {
    const resolvedEvents = [];
    window.addEventListener("latexd:source-jump-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    window.location.hash = "#src=main.tex&line=2&column=4";
    window.dispatchEvent(new Event("hashchange"));
    await flush();

    window.dispatchEvent(new Event("hashchange"));
    await flush();

    const jumpCalls = fetchCalls.filter((call) =>
      call.url.startsWith("http://example.test/api/source-jump/15?")
    );
    assert.equal(jumpCalls.length, 1);
    assert.equal(resolvedEvents.length, 1);
    assert.equal(resolvedEvents[0].sourceHash, "#src=main.tex&line=2&column=4");
  });
});

test("viewer hashchange source hash waits for first applied revision before flushing", async () => {
  await withViewerBrowserHarness({
    stateResponse: {
      current_rev: 0,
      last_applied_rev: 0,
      pdf_url: "",
      page_ids: [],
      page_artifacts: []
    }
  }, async ({ window, fetchCalls, flush, sockets }) => {
    const resolvedEvents = [];
    const failedEvents = [];
    window.addEventListener("latexd:source-jump-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });
    window.addEventListener("latexd:source-jump-failed", (event) => {
      failedEvents.push(event.detail);
    });

    window.location.hash = "#src=main.tex&line=2&column=4";
    window.dispatchEvent(new Event("hashchange"));
    await flush();

    const initialJumpCalls = fetchCalls.filter((call) =>
      call.url.startsWith("http://example.test/api/source-jump/")
    );
    assert.equal(initialJumpCalls.length, 0);

    sockets[0].dispatchEvent(new MessageEvent("message", {
      data: JSON.stringify({
        type: "full_pdf_ready",
        rev: 15,
        pdf_url: "/artifacts/rev/15/main.pdf",
        page_ids: ["page-a"],
        page_artifacts: [
          {
            page_id: "page-a",
            pdf_url: "/artifacts/rev/15/pages/page-a.pdf",
            svg_url: "/artifacts/rev/15/pages/page-a.svg"
          }
        ]
      })
    }));
    await flush();

    const jumpCallsAfterRefresh = fetchCalls.filter((call) =>
      call.url.startsWith("http://example.test/api/source-jump/15?")
    );
    assert.equal(jumpCallsAfterRefresh.length, 1);
    const jumpUrl = new URL(jumpCallsAfterRefresh[0].url);
    assert.equal(jumpUrl.searchParams.get("source_hash"), null);
    assert.equal(jumpUrl.searchParams.get("file"), "main.tex");
    assert.equal(jumpUrl.searchParams.get("line"), "2");
    assert.equal(jumpUrl.searchParams.get("column"), "4");
    assert.equal(resolvedEvents.length, 1);
    assert.equal(resolvedEvents[0].sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(failedEvents.length, 0);

    sockets[0].dispatchEvent(new MessageEvent("message", {
      data: JSON.stringify({
        type: "patch_pages",
        rev: 15,
        ops: []
      })
    }));
    await flush();

    const jumpCallsAfterPatch = fetchCalls.filter((call) =>
      call.url.startsWith("http://example.test/api/source-jump/15?")
    );
    assert.equal(jumpCallsAfterPatch.length, 1);
  });
});

test("viewer repeated identical hashchange before first applied revision flushes once", async () => {
  await withViewerBrowserHarness({
    stateResponse: {
      current_rev: 0,
      last_applied_rev: 0,
      pdf_url: "",
      page_ids: [],
      page_artifacts: []
    }
  }, async ({ window, fetchCalls, flush, sockets }) => {
    const resolvedEvents = [];
    window.addEventListener("latexd:source-jump-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    window.location.hash = "#src=main.tex&line=2&column=4";
    window.dispatchEvent(new Event("hashchange"));
    window.dispatchEvent(new Event("hashchange"));
    await flush();

    const jumpCallsBeforeRefresh = fetchCalls.filter((call) =>
      call.url.startsWith("http://example.test/api/source-jump/")
    );
    assert.equal(jumpCallsBeforeRefresh.length, 0);

    sockets[0].dispatchEvent(new MessageEvent("message", {
      data: JSON.stringify({
        type: "full_pdf_ready",
        rev: 15,
        pdf_url: "/artifacts/rev/15/main.pdf",
        page_ids: ["page-a"],
        page_artifacts: [
          {
            page_id: "page-a",
            pdf_url: "/artifacts/rev/15/pages/page-a.pdf",
            svg_url: "/artifacts/rev/15/pages/page-a.svg"
          }
        ]
      })
    }));
    await flush();

    const jumpCallsAfterRefresh = fetchCalls.filter((call) =>
      call.url.startsWith("http://example.test/api/source-jump/15?")
    );
    assert.equal(jumpCallsAfterRefresh.length, 1);
    assert.equal(resolvedEvents.length, 1);
    assert.equal(resolvedEvents[0].sourceHash, "#src=main.tex&line=2&column=4");
  });
});

test("viewer startup source hash ignores repeated identical hashchange after auto-jump", async () => {
  await withViewerBrowserHarness({
    initialHash: "#src=main.tex&line=2&column=4"
  }, async ({ window, fetchCalls, flush }) => {
    const resolvedEvents = [];
    window.addEventListener("latexd:source-jump-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    const jumpCallsBeforeRepeat = fetchCalls.filter((call) =>
      call.url.startsWith("http://example.test/api/source-jump/15?")
    );
    assert.equal(jumpCallsBeforeRepeat.length, 1);

    window.dispatchEvent(new Event("hashchange"));
    await flush();

    const jumpCallsAfterRepeat = fetchCalls.filter((call) =>
      call.url.startsWith("http://example.test/api/source-jump/15?")
    );
    assert.equal(jumpCallsAfterRepeat.length, 1);
    assert.equal(resolvedEvents.length, 0);
  });
});

test("viewer hashchange source hash ignores repeated identical hashchange after resolution", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush }) => {
    const resolvedEvents = [];
    window.addEventListener("latexd:source-jump-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    window.location.hash = "#src=main.tex&line=2&column=4";
    window.dispatchEvent(new Event("hashchange"));
    await flush();

    const jumpCallsAfterFirstHashchange = fetchCalls.filter((call) =>
      call.url.startsWith("http://example.test/api/source-jump/15?")
    );
    assert.equal(jumpCallsAfterFirstHashchange.length, 1);
    assert.equal(resolvedEvents.length, 1);

    window.dispatchEvent(new Event("hashchange"));
    await flush();

    const jumpCallsAfterSecondHashchange = fetchCalls.filter((call) =>
      call.url.startsWith("http://example.test/api/source-jump/15?")
    );
    assert.equal(jumpCallsAfterSecondHashchange.length, 1);
    assert.equal(resolvedEvents.length, 1);
  });
});

test("viewer select source returns resolved jump detail payload", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush }) => {
    const resolvedEvents = [];
    window.addEventListener("latexd:source-jump-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    const detail = await window.latexdSelectSource({ file: "main.tex", line: 2, column: 4 });
    await flush();

    const jumpCall = fetchCalls.find((call) => call.url.startsWith("http://example.test/api/source-jump/15?"));
    assert.ok(jumpCall);
    assert.equal(detail.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.item?.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.absoluteFile, "/tmp/project/main.tex");
    assert.equal(detail.fileUri, "file:///tmp/project/main.tex");
    assert.equal(detail.line, 2);
    assert.equal(detail.column, 4);
    assert.equal(detail.editorPreviewKind, "none");
    assert.equal(detail.editorLaunchSupported, false);
    assert.equal(detail.response?.page_id, "page-a");
    assert.equal(resolvedEvents.length, 1);
    assert.equal(resolvedEvents[0].sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(resolvedEvents[0].response?.page_id, "page-a");
  });
});

test("viewer hashchange matching selected source hash does not duplicate source jump work", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush }) => {
    const resolvedEvents = [];
    window.addEventListener("latexd:source-jump-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    const detail = await window.latexdSelectSource({ file: "main.tex", line: 2, column: 4 });
    await flush();

    const jumpCallsAfterSelect = fetchCalls.filter((call) =>
      call.url.startsWith("http://example.test/api/source-jump/15?")
    );
    assert.equal(jumpCallsAfterSelect.length, 1);
    assert.equal(detail.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(window.location.hash, "#src=main.tex&line=2&column=4");
    assert.equal(resolvedEvents.length, 1);

    window.dispatchEvent(new Event("hashchange"));
    await flush();

    const jumpCallsAfterHashchange = fetchCalls.filter((call) =>
      call.url.startsWith("http://example.test/api/source-jump/15?")
    );
    assert.equal(jumpCallsAfterHashchange.length, 1);
    assert.equal(resolvedEvents.length, 1);
  });
});

test("viewer explicit column-one hashchange matches omitted selected source hash", async () => {
  await withViewerBrowserHarness({
    sourceJumpResponse: {
      column: 1,
      column0: 0,
      source_hash: ""
    }
  }, async ({ window, fetchCalls, flush }) => {
    const resolvedEvents = [];
    window.addEventListener("latexd:source-jump-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    const detail = await window.latexdSelectSource({ file: "main.tex", line: 2, column: 1 });
    await flush();

    const jumpCallsAfterSelect = fetchCalls.filter((call) =>
      call.url.startsWith("http://example.test/api/source-jump/15?")
    );
    assert.equal(jumpCallsAfterSelect.length, 1);
    assert.equal(detail.column, 1);
    assert.equal(detail.sourceHash, "#src=main.tex&line=2");
    assert.equal(window.location.hash, "#src=main.tex&line=2");
    assert.equal(resolvedEvents.length, 1);

    window.location.hash = "#src=main.tex&line=2&column=1";
    window.dispatchEvent(new Event("hashchange"));
    await flush();

    const jumpCallsAfterEquivalentHashchange = fetchCalls.filter((call) =>
      call.url.startsWith("http://example.test/api/source-jump/15?")
    );
    assert.equal(jumpCallsAfterEquivalentHashchange.length, 1);
    assert.equal(resolvedEvents.length, 1);
  });
});

test("viewer jump accepts canonical source hash direct input", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush }) => {
    const detail = await window.latexdJumpToSource("#src=main.tex&line=2&column=4");
    await flush();

    const jumpCall = fetchCalls.find((call) => call.url.startsWith("http://example.test/api/source-jump/15?"));
    assert.ok(jumpCall);
    const jumpUrl = new URL(jumpCall.url);
    assert.equal(jumpUrl.searchParams.get("source_hash"), "#src=main.tex&line=2&column=4");
    assert.deepEqual(detail.source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    assert.equal(detail.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.line, 2);
    assert.equal(detail.column, 4);
  });
});

test("viewer jump direct object input prefers source hash over inconsistent explicit fields", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush }) => {
    const detail = await window.latexdJumpToSource({
      file: "wrong.tex",
      line: 9,
      column: 1,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    await flush();

    const jumpCall = fetchCalls.find((call) => call.url.startsWith("http://example.test/api/source-jump/15?"));
    assert.ok(jumpCall);
    const jumpUrl = new URL(jumpCall.url);
    assert.equal(jumpUrl.searchParams.get("source_hash"), "#src=main.tex&line=2&column=4");
    assert.equal(jumpUrl.searchParams.get("file"), "main.tex");
    assert.equal(jumpUrl.searchParams.get("line"), "2");
    assert.equal(jumpUrl.searchParams.get("column"), "4");
    assert.deepEqual(detail.source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    assert.equal(detail.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.line, 2);
    assert.equal(detail.column, 4);
  });
});

test("viewer hover source returns resolved hover detail payload", async () => {
  await withViewerBrowserHarness(async ({ window, fetchCalls, flush }) => {
    const resolvedEvents = [];
    window.addEventListener("latexd:source-hover-resolved", (event) => {
      resolvedEvents.push(event.detail);
    });

    const detail = await window.latexdHoverSource({ file: "main.tex", line: 2, column: 4 });
    await flush();

    const jumpCall = fetchCalls.find((call) => call.url.startsWith("http://example.test/api/source-jump/15?"));
    assert.ok(jumpCall);
    assert.equal(detail.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.item?.pageId, "page-a");
    assert.equal(detail.item?.sourceHash, "#src=main.tex&line=2&column=4");
    assert.equal(detail.editorPreviewKind, "none");
    assert.equal(detail.editorLaunchSupported, false);
    assert.equal(detail.response?.page_id, "page-a");
    assert.equal(resolvedEvents.length, 1);
    assert.equal(resolvedEvents[0].item?.pageId, "page-a");
    assert.equal(resolvedEvents[0].sourceHash, "#src=main.tex&line=2&column=4");
  });
});
