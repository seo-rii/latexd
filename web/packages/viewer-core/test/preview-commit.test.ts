import test from "node:test";
import assert from "node:assert/strict";
import { initialState, reduce } from "../src/index.ts";

function commit(base: number, rev: number, ids = ["a", "b"], ops = []) {
  return {
    type: "preview_committed", base_preview_rev: base, new_preview_rev: rev,
    pdf_url: `/artifacts/rev/${rev}/main.pdf`, page_ids: ids,
    page_artifacts: ids.map((id) => ({ page_id: id, pdf_url: `/${id}.pdf` })), ops,
    source_snapshot: [{ file: "main.tex", content: `source ${rev}`, line_count: 1 }],
    render_ir_artifacts: { events_url: `/artifacts/rev/${rev}/events.json` }
  };
}

test("preview commits publish pages and sources together while retaining the selected page", () => {
  let state = reduce(initialState, commit(0, 1));
  state = reduce(state, { type: "ui_page_changed", page: 2 });
  state = reduce(state, { type: "build_started", rev: 2, changed_files: ["main.tex"] });
  assert.equal(state.sourceFiles["main.tex"].content, "source 1");
  state = reduce(state, commit(1, 2, ["x", "a", "b"], [
    { op: "insert_page", index: 0, page_id: "x", pdf_url: "/x.pdf" }
  ]));
  assert.deepEqual(state.pageIds, ["x", "a", "b"]);
  assert.equal(state.currentPage, 3);
  assert.equal(state.lastAppliedRev, 2);
  assert.equal(state.sourceFiles["main.tex"].content, "source 2");
  assert.equal(state.sourceFiles["main.tex"].rev, 2);
  assert.equal(state.renderIrArtifacts.events_url, "/artifacts/rev/2/events.json");
  assert.deepEqual(state.pendingPagePatchOps, []);
});

test("missing base leaves last-good preview intact and requests a fresh manifest", () => {
  const good = reduce(initialState, commit(0, 1));
  const missing = reduce(good, commit(2, 3, ["wrong"]));
  assert.equal(missing.previewResyncRev, 3);
  assert.equal(missing.lastAppliedRev, 1);
  assert.strictEqual(missing.pages, good.pages);
  assert.strictEqual(missing.sourceFiles, good.sourceFiles);
  assert.strictEqual(reduce(missing, { type: "ui_preview_snapshot", snapshot: {
    last_applied_rev: 2, current_rev: 2
  } }), missing);
  const newest = commit(2, 3, ["new"]);
  const recovered = reduce(missing, { type: "ui_preview_snapshot", snapshot: {
    ...newest, current_rev: 4, last_applied_rev: 3, preview_protocol_version: 1,
    building: false, last_build_succeeded: false, diagnostics: [], changed_files: []
  } });
  assert.equal(recovered.previewResyncRev, 0);
  assert.equal(recovered.currentRev, 4);
  assert.equal(recovered.lastAppliedRev, 3);
  assert.deepEqual(recovered.pageIds, ["new"]);
  assert.equal(recovered.sourceFiles["main.tex"].content, "source 3");
});

test("out-of-order build notices do not prevent applying an already committed preview", () => {
  let state = reduce(initialState, commit(0, 1));
  state = reduce(state, { type: "build_started", rev: 3, changed_files: [] });
  state = reduce(state, { type: "build_finished", rev: 3, success: false });
  state = reduce(state, commit(1, 2));
  assert.equal(state.currentRev, 3);
  assert.equal(state.lastAppliedRev, 2);
  assert.equal(state.lastBuildSucceeded, false);
  assert.equal(state.building, false);
  assert.equal(state.sourceFiles["main.tex"].rev, 2);
  assert.strictEqual(reduce(state, commit(0, 1)), state);
  assert.strictEqual(reduce(state, commit(1, 2)), state);
});

test("split legacy messages cannot mutate a committed atomic preview", () => {
  const state = reduce(initialState, commit(0, 1));
  for (const message of [
    { type: "patch_pages", rev: 2, ops: [{ op: "delete_page", index: 0 }] },
    { type: "full_pdf_ready", rev: 2, pdf_url: "/bad.pdf", page_ids: ["wrong"] },
    { type: "source_snapshot", rev: 1, files: [] }
  ]) {
    assert.strictEqual(reduce(state, message), state);
  }
});

test("state resync updates an older preview without rolling back newer diagnostics", () => {
  let state = reduce(initialState, commit(0, 1));
  state = reduce(state, { type: "build_started", rev: 4, changed_files: ["latest.tex"] });
  state = reduce(state, { type: "diagnostics", rev: 4, items: [{ message: "latest failure" }] });
  const latest = commit(1, 3);
  state = reduce(state, { type: "ui_preview_snapshot", snapshot: {
    ...latest, current_rev: 3, last_applied_rev: 3, building: false,
    last_build_succeeded: true, diagnostics: [], changed_files: []
  } });
  assert.equal(state.currentRev, 4);
  assert.equal(state.lastAppliedRev, 3);
  assert.equal(state.building, true);
  assert.equal(state.diagnostics[0].message, "latest failure");
  assert.deepEqual(state.changedFiles, ["latest.tex"]);
});
