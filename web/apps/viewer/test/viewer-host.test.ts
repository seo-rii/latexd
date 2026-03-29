import test from "node:test";
import assert from "node:assert/strict";

import { mountLatexdViewerHost } from "../src/lib/viewer-host.ts";

async function withViewerBrowserHarness(optionsOrRun, maybeRun) {
  const options = typeof optionsOrRun === "function" ? {} : (optionsOrRun ?? {});
  const run = typeof optionsOrRun === "function" ? optionsOrRun : maybeRun;
  const previousWindow = globalThis.window;
  const previousDocument = globalThis.document;
  const previousFetch = globalThis.fetch;
  const previousWebSocket = globalThis.WebSocket;
  const previousCustomEvent = globalThis.CustomEvent;

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
      const wanted = selector.startsWith(".") ? selector.slice(1) : null;
      const queue = [...this.children];
      while (queue.length > 0) {
        const node = queue.shift();
        if (
          wanted
          && String(node.className).split(/\s+/).includes(wanted)
        ) {
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
  const fakeShadowRoot = new FakeNode("#shadow-root");
  fakeShadowRoot.querySelector = (selector) => {
    if (selector.startsWith("#")) {
      return elements.get(selector.slice(1)) ?? null;
    }
    return null;
  };
  const fakeRoot = new FakeNode("div");
  fakeRoot.attachShadow = () => {
    fakeRoot.shadowRoot = fakeShadowRoot;
    return fakeShadowRoot;
  };
  fakeRoot.shadowRoot = null;
  const fakeDocument = {
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
    if (url.pathname === "/api/syncmap/15/page-a") {
      return jsonResponse({
        rev: 15,
        page_id: "page-a",
        page_width_px: 612,
        page_height_px: 792,
        page_source_start_utf8: 5,
        page_source_end_utf8: 20,
        page_output_start_utf8: 32,
        page_output_end_utf8: 64,
        items: [jumpPayload.item]
      });
    }
    if (url.pathname === "/api/tiles/15/page-a") {
      return jsonResponse({
        rev: 15,
        page_id: "page-a",
        tile_size: Number(url.searchParams.get("tile_size") ?? 256),
        items: []
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

    close() {}
  }

  const flush = async () => {
    await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await Promise.resolve();
  };

  const mountTestHost = () => {
    return mountLatexdViewerHost(fakeRoot, {
      window: fakeWindow,
      document: fakeDocument,
      fetch: fakeFetch,
      WebSocket: FakeWebSocket,
      CustomEvent: FakeCustomEvent
    });
  };

  let mountedViewer = null;
  try {
    mountedViewer = mountTestHost();
    await flush();
    return await run({ window: fakeWindow, fetchCalls, flush, sockets });
  } finally {
    mountedViewer?.destroy();
    globalThis.window = previousWindow;
    globalThis.document = previousDocument;
    globalThis.fetch = previousFetch;
    globalThis.WebSocket = previousWebSocket;
    globalThis.CustomEvent = previousCustomEvent;
  }
}

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
    assert.equal(detail.error, "latexd request failed: 500");
    assert.equal(resolvedEvents.length, 0);
    assert.equal(failedEvents.length, 1);
    assert.deepEqual(failedEvents[0].source, {
      file: "main.tex",
      startLine: 2,
      startColumn: 4,
      sourceHash: "#src=main.tex&line=2&column=4"
    });
    assert.equal(failedEvents[0].error, "latexd request failed: 500");
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
    assert.equal(detail.error, "latexd request failed: 500");
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
    assert.equal(detail.error, "latexd request failed: 500");
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
    assert.equal(result.error, "latexd request failed: 500");
    assert.equal(resolvedEvents.length, 0);
    assert.equal(failedEvents.length, 1);
    assert.deepEqual(failedEvents[0].request, {
      file: "main.tex",
      offset: null,
      line: 2,
      column: 4
    });
    assert.equal(failedEvents[0].rev, 15);
    assert.equal(failedEvents[0].error, "latexd request failed: 500");
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
    assert.equal(result.error, "latexd request failed: 500");
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
    assert.equal(result.error, "latexd request failed: 500");
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
    assert.equal(result.error, "latexd request failed: 500");
    assert.equal(resolvedEvents.length, 0);
    assert.equal(failedEvents.length, 1);
    assert.equal(failedEvents[0].error, "latexd request failed: 500");
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
    assert.equal(result.error, "latexd request failed: 500");
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
    assert.equal(result.error, "latexd request failed: 500");
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
