import assert from "node:assert/strict";
import test from "node:test";

import { runBrowserCompiler } from "../src/lib/browser-compiler-runtime.ts";

const encoder = new TextEncoder();

function uleb(value: number) {
  const bytes: number[] = [];
  do {
    let byte = value & 0x7f;
    value >>>= 7;
    if (value !== 0) {
      byte |= 0x80;
    }
    bytes.push(byte);
  } while (value !== 0);
  return bytes;
}

function name(text: string) {
  const bytes = [...encoder.encode(text)];
  return [...uleb(bytes.length), ...bytes];
}

function section(id: number, entries: number[][]) {
  const body = [...uleb(entries.length), ...entries.flat()];
  return [id, ...uleb(body.length), ...body];
}

// A WASI command that writes `stderrText` to fd 2 and then runs `ending`, which is
// raw instruction bytes: a trap, a proc_exit call, or nothing.
function wasiCommand(stderrText: string, ending: number[]) {
  const message = [...encoder.encode(stderrText)];
  const i32 = 0x7f;
  const types = section(1, [
    [0x60, 4, i32, i32, i32, i32, 1, i32],
    [0x60, 1, i32, 0],
    [0x60, 0, 0]
  ]);
  const imports = section(2, [
    [...name("wasi_snapshot_preview1"), ...name("fd_write"), 0x00, 0],
    [...name("wasi_snapshot_preview1"), ...name("proc_exit"), 0x00, 1]
  ]);
  const functions = section(3, [[2]]);
  const memory = section(5, [[0x00, 1]]);
  const exports = section(7, [
    [...name("memory"), 0x02, 0],
    [...name("_start"), 0x00, 2]
  ]);
  const body = [
    0x00,
    0x41, 2, 0x41, 0, 0x41, 1, 0x41, 8, 0x10, 0, 0x1a,
    ...ending,
    0x0b
  ];
  const code = section(10, [[...uleb(body.length), ...body]]);
  // iovec { buf: 16, len } at 0, nwritten at 8, message bytes at 16.
  const header = [16, 0, 0, 0, message.length & 0xff, message.length >> 8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
  const dataSegment = section(11, [[0x00, 0x41, 0, 0x0b, ...uleb(header.length + message.length), ...header, ...message]]);
  const module = new Uint8Array([
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
    ...types, ...imports, ...functions, ...memory, ...exports, ...code, ...dataSegment
  ]);
  return `data:application/wasm;base64,${Buffer.from(module).toString("base64")}`;
}

const project = { "main.tex": encoder.encode("\\documentclass{article}") };

test("a compiler panic reports the panic text instead of a bare trap", async () => {
  const compilerUrl = wasiCommand("thread 'main' panicked at src/main.rs:7:5:\nboom\n", [0x00]);

  await assert.rejects(
    runBrowserCompiler(project, "main.tex", 1, compilerUrl),
    (error: Error) => {
      assert.match(error.message, /crashed/);
      assert.match(error.message, /panicked at src\/main\.rs:7:5:\nboom/);
      return true;
    }
  );
});

test("a non-zero compiler exit reports its code and stderr instead of a JSON parse error", async () => {
  const compilerUrl = wasiCommand("memory allocation failed\n", [0x41, 3, 0x10, 1]);

  await assert.rejects(
    runBrowserCompiler(project, "main.tex", 2, compilerUrl),
    (error: Error) => {
      assert.doesNotMatch(error.message, /JSON/);
      assert.match(error.message, /exited with code 3/);
      assert.match(error.message, /memory allocation failed/);
      return true;
    }
  );
});

test("a compiler that exits cleanly without output.json says so", async () => {
  const compilerUrl = wasiCommand("nothing to do\n", []);

  await assert.rejects(
    runBrowserCompiler(project, "main.tex", 3, compilerUrl),
    (error: Error) => {
      assert.doesNotMatch(error.message, /JSON/);
      assert.match(error.message, /no output\.json/);
      assert.match(error.message, /nothing to do/);
      return true;
    }
  );
});
