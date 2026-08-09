import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { readFile, stat } from "node:fs/promises";
import path from "node:path";
import { promisify } from "node:util";
import {
  brotliCompressSync,
  constants as zlibConstants,
  gzipSync
} from "node:zlib";

const execFileAsync = promisify(execFile);
const ARTIFACT_ENV = "LATEXD_WASM_COST_ARTIFACT";
const MAX_COMPILE_SAMPLES = 20;
const MAX_MEASUREMENT_INPUT_BYTES = 64 * 1024 * 1024;
const COMPILE_TIMEOUT_MS = 30_000;
const COLD_COMPILE_PROBE = `
import { readFile } from "node:fs/promises";
import { performance } from "node:perf_hooks";

const bytes = await readFile(process.env.${ARTIFACT_ENV});
const startedAt = performance.now();
await WebAssembly.compile(bytes);
process.stdout.write(String(performance.now() - startedAt));
`;

async function measureColdCompileSample(artifactPath) {
  const { stdout } = await execFileAsync(
    process.execPath,
    ["--input-type=module", "--eval", COLD_COMPILE_PROBE],
    {
      env: {
        ...process.env,
        [ARTIFACT_ENV]: artifactPath
      },
      maxBuffer: 1024,
      timeout: COMPILE_TIMEOUT_MS,
      killSignal: "SIGKILL"
    }
  );
  const elapsedMs = Number(stdout.trim());
  if (!Number.isFinite(elapsedMs) || elapsedMs < 0) {
    throw new Error(`invalid cold WebAssembly.compile duration: ${stdout}`);
  }
  return elapsedMs;
}

export async function measureWasmCost(
  artifactPath,
  {
    compileSamples = 3,
    maxRawBytes = MAX_MEASUREMENT_INPUT_BYTES
  } = {}
) {
  if (
    !Number.isSafeInteger(compileSamples)
    || compileSamples < 0
    || compileSamples > MAX_COMPILE_SAMPLES
  ) {
    throw new Error(`compileSamples must be between 0 and ${MAX_COMPILE_SAMPLES}`);
  }
  if (!Number.isSafeInteger(maxRawBytes) || maxRawBytes < 0) {
    throw new Error("maxRawBytes must be a non-negative safe integer");
  }

  const artifactStat = await stat(artifactPath);
  if (!artifactStat.isFile()) {
    throw new Error(`WASI cost artifact is not a regular file: ${artifactPath}`);
  }
  if (artifactStat.size > maxRawBytes) {
    throw new Error(
      `raw byte budget exceeded before measurement: actual=${artifactStat.size} maximum=${maxRawBytes}`
    );
  }

  const bytes = await readFile(artifactPath);
  if (bytes.byteLength > maxRawBytes) {
    throw new Error(
      `raw byte budget exceeded during measurement: actual=${bytes.byteLength} maximum=${maxRawBytes}`
    );
  }
  const samples = [];
  for (let index = 0; index < compileSamples; index += 1) {
    samples.push(await measureColdCompileSample(artifactPath));
  }
  const sortedSamples = [...samples].sort((left, right) => left - right);
  const middle = Math.floor(sortedSamples.length / 2);
  const median = sortedSamples.length === 0
    ? null
    : sortedSamples.length % 2 === 0
      ? (sortedSamples[middle - 1] + sortedSamples[middle]) / 2
      : sortedSamples[middle];

  return {
    schema_version: 1,
    artifact: path.basename(artifactPath),
    sha256: createHash("sha256").update(bytes).digest("hex"),
    raw_bytes: bytes.byteLength,
    gzip_bytes: gzipSync(bytes, { level: 9 }).byteLength,
    brotli_bytes: brotliCompressSync(bytes, {
      params: {
        [zlibConstants.BROTLI_PARAM_QUALITY]: 11
      }
    }).byteLength,
    cold_compile_ms: {
      sample_count: samples.length,
      samples,
      median
    },
    environment: {
      node: process.version,
      platform: process.platform,
      arch: process.arch
    }
  };
}

export function assertWasmCostWithinBudget(report, budget) {
  if (report.schema_version !== 1) {
    throw new Error(`unsupported WASI cost report schema: ${report.schema_version}`);
  }
  if (budget.schema_version !== 1) {
    throw new Error(`unsupported WASI cost budget schema: ${budget.schema_version}`);
  }
  if (report.artifact !== budget.artifact) {
    throw new Error(
      `WASI cost artifact mismatch: report=${report.artifact} budget=${budget.artifact}`
    );
  }
  if (typeof report.sha256 !== "string" || !/^[0-9a-f]{64}$/.test(report.sha256)) {
    throw new Error("WASI cost report is missing a valid SHA-256 artifact identity");
  }

  for (const [budgetField, reportField] of [
    ["max_raw_bytes", "raw_bytes"],
    ["max_gzip_bytes", "gzip_bytes"],
    ["max_brotli_bytes", "brotli_bytes"]
  ]) {
    const maximum = budget[budgetField];
    const actual = report[reportField];
    if (!Number.isSafeInteger(maximum) || maximum < 0) {
      throw new Error(`invalid WASI cost budget field ${budgetField}: ${maximum}`);
    }
    if (!Number.isSafeInteger(actual) || actual < 0) {
      throw new Error(`invalid WASI cost report field ${reportField}: ${actual}`);
    }
    if (actual > maximum) {
      throw new Error(`${budgetField} exceeded: actual=${actual} maximum=${maximum}`);
    }
  }
}
