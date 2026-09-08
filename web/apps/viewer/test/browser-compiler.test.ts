import assert from "node:assert/strict";
import test from "node:test";

import { BrowserCompilerClient, type BrowserCompilerWorker } from "../src/lib/browser-compiler-client.ts";
import type { BrowserCompileRequest, BrowserCompileResponse, BrowserCompileResult } from "../src/lib/browser-compiler-protocol.ts";

class ControlledWorker implements BrowserCompilerWorker {
  onmessage: BrowserCompilerWorker["onmessage"] = null;
  onerror: BrowserCompilerWorker["onerror"] = null;
  onmessageerror: BrowserCompilerWorker["onmessageerror"] = null;
  requests: BrowserCompileRequest[] = [];
  terminated = false;
  postError: Error | null = null;

  postMessage(message: BrowserCompileRequest, transfer: Transferable[]) {
    if (this.postError) throw this.postError;
    this.requests.push(structuredClone(message, { transfer }));
  }

  terminate() { this.terminated = true; }

  respond(response: BrowserCompileResponse) {
    this.onmessage?.({ data: response } as MessageEvent<BrowserCompileResponse>);
  }

  complete(revision: number, requestId = this.requests.at(-1)!.requestId) {
    const result = {
      extracted_text: `revision ${revision}`,
      build_metadata: { revision },
      pdf: new Uint8Array([37, 80, 68, 70])
    } as BrowserCompileResult;
    this.respond({ type: "compiled", requestId, revision, result });
  }
}

function setup(timeoutMs = 5_000) {
  const workers: ControlledWorker[] = [];
  const client = new BrowserCompilerClient(() => {
    const worker = new ControlledWorker();
    workers.push(worker);
    return worker;
  }, "/latexd/wasi/latexd-wasi.wasm", timeoutMs);
  return { workers, client };
}

test("compiler sends an immutable transferable snapshot and preserves editor buffers", async () => {
  const { workers, client } = setup();
  const bytes = new Uint8Array([65, 66]);
  const pending = client.compileProject({ "main.tex": bytes }, "main.tex", 1);
  assert.equal(workers.length, 1);
  bytes[0] = 90;
  assert.deepEqual([...workers[0].requests[0].files["main.tex"]], [65, 66]);
  assert.deepEqual([...bytes], [90, 66]);
  assert.equal(workers[0].requests[0].compilerUrl, "/latexd/wasi/latexd-wasi.wasm");
  workers[0].complete(1);
  assert.equal((await pending).extracted_text, "revision 1");
  client.dispose();
});

test("idle worker is reused without sharing a previous compilation result", async () => {
  const { workers, client } = setup();
  const first = client.compileSource("first", 1);
  workers[0].complete(1);
  await first;
  const second = client.compileSource("second", 2);
  assert.equal(workers.length, 1);
  assert.equal(workers[0].requests.length, 2);
  workers[0].complete(2);
  assert.equal((await second).build_metadata.revision, 2);
  client.dispose();
});

test("superseding a running compile terminates it and rejects even queued stale responses", async () => {
  const { workers, client } = setup();
  const first = client.compileSource("obsolete", 1);
  const rejected = assert.rejects(first, { name: "AbortError" });
  const staleHandler = workers[0].onmessage!;
  const second = client.compileSource("latest", 2);
  assert.equal(workers[0].terminated, true);
  assert.equal(workers.length, 2);
  await rejected;
  let settled = false;
  void second.then(() => { settled = true; });
  staleHandler({ data: {
    type: "failed", requestId: workers[1].requests[0].requestId, revision: 2,
    error: "late event from old worker"
  } } as MessageEvent<BrowserCompileResponse>);
  workers[1].complete(1, workers[0].requests[0].requestId);
  await Promise.resolve();
  assert.equal(settled, false);
  workers[1].complete(2);
  assert.equal((await second).build_metadata.revision, 2);
  client.dispose();
});

test("AbortSignal interrupts the worker and the next compile starts cleanly", async () => {
  const { workers, client } = setup();
  const controller = new AbortController();
  const first = client.compileSource("cancel", 1, { signal: controller.signal });
  const rejected = assert.rejects(first, { name: "AbortError" });
  controller.abort();
  await rejected;
  assert.equal(workers[0].terminated, true);
  const second = client.compileSource("recover", 2);
  workers[1].complete(2);
  await second;
  client.dispose();
});

test("an already aborted request does not replace an active request", async () => {
  const { workers, client } = setup();
  const first = client.compileSource("active", 1);
  await assert.rejects(client.compileSource("cancelled", 2, {
    signal: AbortSignal.abort()
  }), { name: "AbortError" });
  assert.equal(workers.length, 1);
  assert.equal(workers[0].terminated, false);
  workers[0].complete(1);
  await first;
  client.dispose();
});

test("completed requests detach their abort handlers", async () => {
  const { workers, client } = setup();
  const controller = new AbortController();
  const first = client.compileSource("first", 1, { signal: controller.signal });
  workers[0].complete(1);
  await first;
  const second = client.compileSource("second", 2);
  controller.abort();
  assert.equal(workers[0].terminated, false);
  workers[0].complete(2);
  await second;
  client.dispose();
});

test("worker crash rejects the request and recovery creates a fresh worker", async () => {
  const { workers, client } = setup();
  const first = client.compileSource("crash", 1);
  const rejected = assert.rejects(first, /worker crashed/);
  workers[0].onerror?.({ message: "worker crashed", preventDefault() {} } as ErrorEvent);
  await rejected;
  assert.equal(workers[0].terminated, true);
  const second = client.compileSource("recover", 2);
  workers[1].complete(2);
  await second;
  client.dispose();
});

test("compilation and decoding failures reject without poisoning the next worker", async () => {
  const { workers, client } = setup();
  const first = client.compileSource("bad input", 1);
  workers[0].respond({ type: "failed", requestId: 1, revision: 1, error: "TeX error" });
  await assert.rejects(first, /TeX error/);
  const second = client.compileSource("bad response", 2);
  workers[1].onmessageerror?.({} as MessageEvent);
  await assert.rejects(second, /could not be decoded/);
  const third = client.compileSource("recovered", 3);
  workers[2].complete(3);
  await third;
  client.dispose();
});

test("response envelope and artifact revision must agree with the pending request", async () => {
  const { workers, client } = setup();
  const first = client.compileSource("expected 1", 1);
  workers[0].complete(2);
  await assert.rejects(first, /different revision/);
  const second = client.compileSource("expected 2", 2);
  workers[1].respond({
    type: "compiled", requestId: 2, revision: 2,
    result: { build_metadata: { revision: 1 } } as BrowserCompileResult
  });
  await assert.rejects(second, /invalid build metadata/);
  client.dispose();
});

test("hung workers time out, terminate, and allow another build", async () => {
  const { workers, client } = setup(10);
  await assert.rejects(client.compileSource("hang", 1), /timed out/);
  assert.equal(workers[0].terminated, true);
  const next = client.compileSource("recover", 2);
  workers[1].complete(2);
  await next;
  client.dispose();
});

test("dispose terminates pending and idle workers and prohibits subsequent use", async () => {
  const { workers, client } = setup();
  const pending = client.compileSource("running", 1);
  client.dispose();
  await assert.rejects(pending, { name: "AbortError" });
  assert.equal(workers[0].terminated, true);
  await assert.rejects(client.compileSource("after dispose", 2), /disposed/);
  assert.equal(workers.length, 1);
  client.dispose();
});

test("worker startup failures reject asynchronously and can be retried", async () => {
  const worker = new ControlledWorker();
  let attempts = 0;
  const client = new BrowserCompilerClient(() => {
    if (attempts++ === 0) throw new Error("Worker startup failed");
    return worker;
  }, "/compiler.wasm");
  await assert.rejects(client.compileSource("first", 1), /startup failed/);
  const second = client.compileSource("second", 2);
  worker.complete(2);
  await second;
  client.dispose();
});
