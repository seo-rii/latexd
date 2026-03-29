import test from "node:test";
import assert from "node:assert/strict";

import { createLatexdApiClient } from "../src/lib/latexd-client.ts";

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
