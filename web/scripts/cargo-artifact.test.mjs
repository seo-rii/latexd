import assert from "node:assert/strict";
import test from "node:test";

import { cargoArtifactFile } from "./cargo-artifact.mjs";

const artifactMessage = (name, filenames) => JSON.stringify({
  reason: "compiler-artifact",
  target: { name, kind: ["bin"] },
  filenames
});

test("the WASI artifact path comes from cargo, not from <repo>/target", () => {
  const stdout = [
    artifactMessage("tex-vm", ["/shared/target/wasm32-wasip1/release/deps/libtex_vm.rlib"]),
    artifactMessage("latexd-wasi", ["/shared/target/wasm32-wasip1/release/latexd-wasi.wasm"]),
    JSON.stringify({ reason: "build-finished", success: true }),
    ""
  ].join("\n");
  assert.equal(
    cargoArtifactFile(stdout, "latexd-wasi", ".wasm"),
    "/shared/target/wasm32-wasip1/release/latexd-wasi.wasm"
  );
});

test("a build that reports no matching artifact is an error", () => {
  const stdout = `${artifactMessage("tex-vm", ["/t/libtex_vm.rlib"])}\nwarning: not json\n`;
  assert.throws(
    () => cargoArtifactFile(stdout, "latexd-wasi", ".wasm"),
    /cargo did not report a \.wasm artifact for latexd-wasi/
  );
});
