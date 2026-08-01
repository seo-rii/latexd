import assert from "node:assert/strict";
import test from "node:test";

import type { BrowserSourceProvenance } from "../src/lib/browser-artifacts.ts";
import {
  browserSourceKey,
  resolveBrowserSourceSelection
} from "../src/lib/display-list-source.ts";

const encoder = new TextEncoder();

function fileProvenance(
  path: string,
  startUtf8: number,
  endUtf8: number
): BrowserSourceProvenance {
  return {
    primary: {
      kind: "file",
      path,
      start_utf8: startUtf8,
      end_utf8: endUtf8
    },
    related: [],
    expansion_stack: [],
    generated_by: "source",
    expansion_stack_truncated: false
  };
}

test("display-list source spans map UTF-8 bytes to textarea UTF-16 ranges", () => {
  const source = "α\nHello 🌍 tail";
  const helloStart = encoder.encode("α\n").byteLength;
  const helloEnd = encoder.encode("α\nHello").byteLength;
  const emojiStart = encoder.encode("α\nHello ").byteLength;
  const emojiEnd = encoder.encode("α\nHello 🌍").byteLength;

  assert.deepEqual(
    resolveBrowserSourceSelection(
      fileProvenance("main.tex", helloStart, helloEnd),
      { "main.tex": source }
    ),
    {
      key: `main.tex:${helloStart}:${helloEnd}`,
      path: "main.tex",
      start_utf8: helloStart,
      end_utf8: helloEnd,
      start_index: 2,
      end_index: 7,
      line: 2,
      column: 1,
      end_line: 2,
      end_column: 6
    }
  );
  assert.deepEqual(
    resolveBrowserSourceSelection(
      fileProvenance("main.tex", emojiStart, emojiEnd),
      { "main.tex": source }
    ),
    {
      key: `main.tex:${emojiStart}:${emojiEnd}`,
      path: "main.tex",
      start_utf8: emojiStart,
      end_utf8: emojiEnd,
      start_index: 8,
      end_index: 10,
      line: 2,
      column: 7,
      end_line: 2,
      end_column: 9
    }
  );
});

test("display-list source selection rejects generated, missing, and invalid spans", () => {
  const generated: BrowserSourceProvenance = {
    primary: {
      kind: "generated",
      stable_id: "synthetic-title",
      description: "generated title punctuation"
    },
    related: [],
    expansion_stack: [],
    generated_by: "layout",
    expansion_stack_truncated: false
  };

  assert.equal(resolveBrowserSourceSelection(generated, { "main.tex": "Body" }), null);
  assert.equal(browserSourceKey(generated), null);
  assert.equal(
    resolveBrowserSourceSelection(fileProvenance("missing.tex", 0, 1), {
      "main.tex": "Body"
    }),
    null
  );
  assert.equal(
    resolveBrowserSourceSelection(fileProvenance("main.tex", 4, 2), {
      "main.tex": "Body"
    }),
    null
  );
  assert.equal(
    browserSourceKey({ primary: null } as unknown as BrowserSourceProvenance),
    null
  );
});
