import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { promisify } from "node:util";

import {
  assertWasmCostWithinBudget,
  measureWasmCost
} from "./wasm-cost.mjs";

const MINIMAL_WASM_MODULE = Uint8Array.from([
  0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00
]);
const execFileAsync = promisify(execFile);

test("WASI cost measurement records compressed sizes and cold compile samples", async () => {
  const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), "latexd-wasm-cost-"));
  const artifactPath = path.join(temporaryRoot, "fixture.wasm");

  try {
    await writeFile(artifactPath, MINIMAL_WASM_MODULE);
    const report = await measureWasmCost(artifactPath, { compileSamples: 2 });

    assert.equal(report.schema_version, 1);
    assert.equal(report.artifact, "fixture.wasm");
    assert.match(report.sha256, /^[0-9a-f]{64}$/);
    assert.equal(report.raw_bytes, MINIMAL_WASM_MODULE.byteLength);
    assert.ok(report.gzip_bytes > 0);
    assert.ok(report.brotli_bytes > 0);
    assert.equal(report.cold_compile_ms.samples.length, 2);
    assert.equal(report.cold_compile_ms.sample_count, 2);
    assert.ok(report.cold_compile_ms.samples.every(Number.isFinite));
    assert.ok(report.cold_compile_ms.samples.every((sample) => sample >= 0));
    assert.ok(Number.isFinite(report.cold_compile_ms.median));
    assert.equal(report.environment.node, process.version);
    assert.equal(report.environment.platform, process.platform);
    assert.equal(report.environment.arch, process.arch);
  } finally {
    await rm(temporaryRoot, { recursive: true, force: true });
  }
});

test("WASI cost measurement supports size-only gates and rejects unsafe work", async () => {
  const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), "latexd-wasm-cost-limits-"));
  const artifactPath = path.join(temporaryRoot, "fixture.wasm");

  try {
    await writeFile(artifactPath, MINIMAL_WASM_MODULE);
    const sizeOnlyReport = await measureWasmCost(artifactPath, {
      compileSamples: 0,
      maxRawBytes: MINIMAL_WASM_MODULE.byteLength
    });
    assert.deepEqual(sizeOnlyReport.cold_compile_ms, {
      sample_count: 0,
      samples: [],
      median: null
    });

    await assert.rejects(
      measureWasmCost(artifactPath, {
        compileSamples: 0,
        maxRawBytes: MINIMAL_WASM_MODULE.byteLength - 1
      }),
      /raw byte budget exceeded before measurement/
    );
    await assert.rejects(
      measureWasmCost(artifactPath, { compileSamples: 21 }),
      /compileSamples must be between 0 and 20/
    );
  } finally {
    await rm(temporaryRoot, { recursive: true, force: true });
  }
});

test("WASI cost budgets reject each deterministic size regression", () => {
  const report = {
    schema_version: 1,
    artifact: "latexd-wasi.wasm",
    sha256: "0".repeat(64),
    raw_bytes: 100,
    gzip_bytes: 80,
    brotli_bytes: 70,
    cold_compile_ms: { sample_count: 1, samples: [1], median: 1 },
    environment: { node: "test", platform: "test", arch: "test" }
  };

  assert.doesNotThrow(() => assertWasmCostWithinBudget(report, {
    schema_version: 1,
    artifact: "latexd-wasi.wasm",
    sha256: "0".repeat(64),
    max_raw_bytes: 100,
    max_gzip_bytes: 80,
    max_brotli_bytes: 70
  }));

  for (const field of ["max_raw_bytes", "max_gzip_bytes", "max_brotli_bytes"]) {
    const budget = {
      schema_version: 1,
      artifact: "latexd-wasi.wasm",
      max_raw_bytes: 100,
      max_gzip_bytes: 80,
      max_brotli_bytes: 70,
      [field]: 1
    };

    assert.throws(
      () => assertWasmCostWithinBudget(report, budget),
      new RegExp(`${field} exceeded`)
    );
  }
});

test("WASI cost budgets reject mismatched artifacts and unsupported schemas", () => {
  const report = {
    schema_version: 1,
    artifact: "latexd-wasi.wasm",
    raw_bytes: 100,
    gzip_bytes: 80,
    brotli_bytes: 70,
    cold_compile_ms: { sample_count: 1, samples: [1], median: 1 },
    environment: { node: "test", platform: "test", arch: "test" }
  };

  assert.throws(
    () => assertWasmCostWithinBudget(report, {
      schema_version: 2,
      artifact: "latexd-wasi.wasm",
      max_raw_bytes: 100,
      max_gzip_bytes: 80,
      max_brotli_bytes: 70
    }),
    /unsupported WASI cost budget schema/
  );
  assert.throws(
    () => assertWasmCostWithinBudget(report, {
      schema_version: 1,
      artifact: "other.wasm",
      max_raw_bytes: 100,
      max_gzip_bytes: 80,
      max_brotli_bytes: 70
    }),
    /artifact mismatch/
  );
});

test("WASI cost CLI validates a budget and writes the reported evidence", async () => {
  const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), "latexd-wasm-cost-cli-"));
  const artifactPath = path.join(temporaryRoot, "fixture.wasm");
  const budgetPath = path.join(temporaryRoot, "budget.json");
  const reportPath = path.join(temporaryRoot, "report.json");

  try {
    await writeFile(artifactPath, MINIMAL_WASM_MODULE);
    await writeFile(budgetPath, `${JSON.stringify({
      schema_version: 1,
      artifact: "fixture.wasm",
      max_raw_bytes: 100,
      max_gzip_bytes: 100,
      max_brotli_bytes: 100
    })}\n`);

    const { stdout } = await execFileAsync(process.execPath, [
      "scripts/report-wasi-cost.mjs",
      artifactPath,
      "--budget",
      budgetPath,
      "--output",
      reportPath,
      "--compile-samples",
      "1"
    ], {
      cwd: path.resolve(import.meta.dirname, ".."),
      maxBuffer: 1024 * 1024
    });

    const stdoutReport = JSON.parse(stdout);
    const storedReport = JSON.parse(await readFile(reportPath, "utf8"));
    assert.deepEqual(storedReport, stdoutReport);
    assert.equal(storedReport.artifact, "fixture.wasm");
    assert.equal(storedReport.cold_compile_ms.sample_count, 1);
  } finally {
    await rm(temporaryRoot, { recursive: true, force: true });
  }
});

test("WASI cost CLI preserves a full report when the budget fails", async () => {
  const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), "latexd-wasm-cost-failure-"));
  const artifactPath = path.join(temporaryRoot, "fixture.wasm");
  const budgetPath = path.join(temporaryRoot, "budget.json");
  const reportPath = path.join(temporaryRoot, "report.json");

  try {
    await writeFile(artifactPath, MINIMAL_WASM_MODULE);
    await writeFile(budgetPath, `${JSON.stringify({
      schema_version: 1,
      artifact: "fixture.wasm",
      max_raw_bytes: 7,
      max_gzip_bytes: 100,
      max_brotli_bytes: 100
    })}\n`);

    await assert.rejects(execFileAsync(process.execPath, [
      "scripts/report-wasi-cost.mjs",
      artifactPath,
      "--budget",
      budgetPath,
      "--output",
      reportPath,
      "--compile-samples",
      "0"
    ], {
      cwd: path.resolve(import.meta.dirname, ".."),
      maxBuffer: 1024 * 1024
    }));

    const storedReport = JSON.parse(await readFile(reportPath, "utf8"));
    assert.equal(storedReport.raw_bytes, MINIMAL_WASM_MODULE.byteLength);
    assert.match(storedReport.sha256, /^[0-9a-f]{64}$/);
  } finally {
    await rm(temporaryRoot, { recursive: true, force: true });
  }
});
