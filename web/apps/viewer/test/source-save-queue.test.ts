import assert from "node:assert/strict";
import test from "node:test";

import { createSourceSaveQueue } from "../src/lib/source-save-queue.ts";

function controlledWriter() {
  const writes: Array<{
    file: string;
    content: string;
    resolve: (value: string) => void;
    reject: (error: Error) => void;
  }> = [];
  const write = (file: string, content: string) => new Promise<string>((resolve, reject) => {
    writes.push({ file, content, resolve, reject });
  });
  return { writes, write };
}

const settle = () => new Promise((resolve) => setImmediate(resolve));

test("a save to a file waits for the write already in flight", async () => {
  const { writes, write } = controlledWriter();
  const saves = createSourceSaveQueue(write);

  const first = saves.save("main.tex", "A");
  const second = saves.save("main.tex", "AB");
  await settle();
  assert.deepEqual(writes.map((entry) => entry.content), ["A"]);

  writes[0].resolve("wrote A");
  assert.equal(await first, "wrote A");
  await settle();
  assert.deepEqual(writes.map((entry) => entry.content), ["A", "AB"]);
  writes[1].resolve("wrote AB");
  assert.equal(await second, "wrote AB");
});

test("saves waiting behind a write coalesce to the newest content", async () => {
  const { writes, write } = controlledWriter();
  const saves = createSourceSaveQueue(write);

  const first = saves.save("main.tex", "A");
  const stale = saves.save("main.tex", "AB");
  const latest = saves.save("main.tex", "ABC");
  writes[0].resolve("wrote A");
  await first;
  await settle();

  assert.deepEqual(writes.map((entry) => entry.content), ["A", "ABC"]);
  writes[1].resolve("wrote ABC");
  assert.equal(await stale, "wrote ABC");
  assert.equal(await latest, "wrote ABC");
});

test("a failed write rejects its callers and newer waiting content is still written", async () => {
  const { writes, write } = controlledWriter();
  const saves = createSourceSaveQueue(write);

  const failed = saves.save("main.tex", "A");
  const next = saves.save("main.tex", "AB");
  writes[0].reject(new Error("disk full"));
  await assert.rejects(failed, /disk full/);
  await settle();

  assert.deepEqual(writes.map((entry) => entry.content), ["A", "AB"]);
  writes[1].resolve("wrote AB");
  assert.equal(await next, "wrote AB");
});

test("saving the content already being written does not write it twice", async () => {
  const { writes, write } = controlledWriter();
  const saves = createSourceSaveQueue(write);

  const first = saves.save("main.tex", "A");
  const repeat = saves.save("main.tex", "A");
  writes[0].resolve("wrote A");

  assert.equal(await first, "wrote A");
  assert.equal(await repeat, "wrote A");
  await settle();
  assert.equal(writes.length, 1);
});

test("different files are written independently", async () => {
  const { writes, write } = controlledWriter();
  const saves = createSourceSaveQueue(write);

  const main = saves.save("main.tex", "A");
  const intro = saves.save("intro.tex", "B");
  await settle();

  assert.deepEqual(writes.map((entry) => `${entry.file}:${entry.content}`), ["main.tex:A", "intro.tex:B"]);
  writes[1].resolve("wrote B");
  writes[0].resolve("wrote A");
  assert.equal(await intro, "wrote B");
  assert.equal(await main, "wrote A");

  const again = saves.save("main.tex", "AA");
  await settle();
  assert.equal(writes.at(-1)?.content, "AA");
  writes.at(-1)?.resolve("wrote AA");
  assert.equal(await again, "wrote AA");
});
