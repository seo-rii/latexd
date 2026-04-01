import test from "node:test";
import assert from "node:assert/strict";

import {
  createLatexdApiClient,
  createLatexdViewerTransport
} from "../src/lib/latexd-client.ts";
import { createLatexdViewerRealtime } from "../src/lib/viewer-socket.ts";

test("latexd api client fetches source file lists and writes source text", async () => {
  const calls: Array<{ url: string; init?: RequestInit }> = [];
  const client = createLatexdApiClient({
    window: {
      location: new URL("http://example.test/")
    } as Window & typeof globalThis,
    fetch: async (input, init) => {
      calls.push({
        url: String(input),
        init
      });
      return {
        ok: true,
        status: 200,
        async json() {
          if (String(input).includes("/api/source-files/")) {
            return {
              rev: 0,
              files: ["main.tex"]
            };
          }
          return {
            file: "main.tex",
            line_count: 2,
            byte_len: 16
          };
        }
      } as Response;
    }
  });

  const files = await client.fetchSourceFiles({ rev: 0 });
  const updated = await client.updateSourceFile({
    file: "main.tex",
    content: "\\section{Hello}\n"
  });

  assert.deepEqual(files, {
    rev: 0,
    files: ["main.tex"]
  });
  assert.deepEqual(updated, {
    file: "main.tex",
    line_count: 2,
    byte_len: 16
  });
  assert.equal(calls[0]?.url, "http://example.test/api/source-files/0");
  assert.equal(calls[1]?.url, "http://example.test/api/source-file");
  assert.equal(calls[1]?.init?.method, "PUT");
  assert.equal(calls[1]?.init?.headers?.["content-type"], "application/json");
  assert.equal(
    calls[1]?.init?.body,
    JSON.stringify({
      file: "main.tex",
      content: "\\section{Hello}\n"
    })
  );
});

test("latexd api client resolves endpoints relative to the current base path", async () => {
  const calls: string[] = [];
  const client = createLatexdApiClient({
    window: {
      location: new URL("http://example.test/viewer/")
    } as Window & typeof globalThis,
    fetch: async (input) => {
      calls.push(String(input));
      return {
        ok: true,
        status: 200,
        async json() {
          return {
            rev: 3,
            files: ["main.tex"]
          };
        }
      } as Response;
    }
  });

  await client.fetchState();
  await client.fetchSourceFiles({ rev: 3 });

  assert.deepEqual(calls, [
    "http://example.test/viewer/api/state",
    "http://example.test/viewer/api/source-files/3"
  ]);
});

test("latexd viewer transport can reuse an app-owned websocket", () => {
  const socket = {
    readyState: 1,
    send() {},
    close() {}
  };
  let openCalls = 0;
  const transport = createLatexdViewerTransport({
    window: {
      location: new URL("http://example.test/viewer/")
    } as Window & typeof globalThis,
    fetch: async () => {
      throw new Error("not expected");
    },
    openWebSocket() {
      openCalls += 1;
      return socket;
    }
  });

  assert.equal(transport.openWebSocket(), socket);
  assert.equal(openCalls, 1);
});

test("latexd viewer realtime exposes websocket lifecycle through a store", () => {
  class FakeWebSocket extends EventTarget {
    readyState = 0;
    url: string;

    constructor(url: string) {
      super();
      this.url = url;
    }

    send() {}

    close() {
      this.readyState = 3;
      this.dispatchEvent(new Event("close"));
    }
  }

  const realtime = createLatexdViewerRealtime({
    window: {
      location: new URL("http://example.test/viewer/")
    } as Window & typeof globalThis,
    WebSocket: FakeWebSocket as unknown as typeof WebSocket
  });
  const socket = realtime.openWebSocket() as FakeWebSocket;
  const seenPhases: string[] = [];
  const unsubscribe = realtime.status.subscribe((status) => {
    seenPhases.push(status.phase);
  });

  socket.readyState = 1;
  socket.dispatchEvent(new Event("open"));
  realtime.destroy();
  unsubscribe();

  assert.equal(socket.url, "ws://example.test/viewer/ws");
  assert.deepEqual(seenPhases, ["connecting", "open", "closed"]);
});

test("latexd viewer realtime exposes parsed websocket messages through a store", () => {
  class FakeWebSocket extends EventTarget {
    readyState = 1;
    url: string;

    constructor(url: string) {
      super();
      this.url = url;
    }

    send() {}

    close() {
      this.readyState = 3;
      this.dispatchEvent(new Event("close"));
    }
  }

  const realtime = createLatexdViewerRealtime({
    window: {
      location: new URL("http://example.test/viewer/")
    } as Window & typeof globalThis,
    WebSocket: FakeWebSocket as unknown as typeof WebSocket
  });
  const socket = realtime.openWebSocket() as FakeWebSocket;
  const seenMessages: unknown[] = [];
  const unsubscribe = realtime.messages.subscribe((message) => {
    if (message) {
      seenMessages.push(message);
    }
  });

  const buildStartedEvent = new Event("message");
  Object.defineProperty(buildStartedEvent, "data", {
    value: JSON.stringify({
      type: "build_started",
      rev: 12,
      changed_files: ["main.tex"]
    })
  });
  socket.dispatchEvent(buildStartedEvent);

  const invalidEvent = new Event("message");
  Object.defineProperty(invalidEvent, "data", {
    value: "not json"
  });
  socket.dispatchEvent(invalidEvent);

  const sourceSnapshotEvent = new Event("message");
  Object.defineProperty(sourceSnapshotEvent, "data", {
    value: JSON.stringify({
      type: "source_snapshot",
      rev: 12,
      files: [{
        file: "main.tex",
        content: "\\section{Hi}\\n",
        line_count: 1
      }]
    })
  });
  socket.dispatchEvent(sourceSnapshotEvent);

  unsubscribe();
  realtime.destroy();

  assert.deepEqual(seenMessages, [
    {
      type: "build_started",
      rev: 12,
      changed_files: ["main.tex"]
    },
    {
      type: "source_snapshot",
      rev: 12,
      files: [{
        file: "main.tex",
        content: "\\section{Hi}\\n",
        line_count: 1
      }]
    }
  ]);
});
