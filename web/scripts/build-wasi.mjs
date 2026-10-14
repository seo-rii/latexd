import { cp, mkdir } from "node:fs/promises";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { cargoArtifactFile } from "./cargo-artifact.mjs";

const webRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = path.resolve(webRoot, "..");

// Ask cargo where it wrote the module: CARGO_TARGET_DIR or a configured
// target-dir moves it away from <repo>/target.
const artifactPath = await new Promise((resolve, reject) => {
  const child = spawn("cargo", [
    "build",
    "--manifest-path",
    path.join(repoRoot, "Cargo.toml"),
    "--package",
    "latexd-wasi",
    "--target",
    "wasm32-wasip1",
    "--release",
    "--message-format=json-render-diagnostics"
  ], { stdio: ["ignore", "pipe", "inherit"] });
  let stdout = "";
  child.stdout.setEncoding("utf8");
  child.stdout.on("data", (chunk) => {
    stdout += chunk;
  });
  child.on("error", reject);
  child.on("close", (code) => {
    if (code !== 0) {
      reject(new Error(`cargo exited with ${code}`));
      return;
    }
    try {
      resolve(cargoArtifactFile(stdout, "latexd-wasi", ".wasm"));
    } catch (error) {
      reject(error);
    }
  });
});

const outputDir = path.join(webRoot, "apps/viewer/static/wasi");
await mkdir(outputDir, { recursive: true });

await new Promise((resolve, reject) => {
  const child = spawn(process.execPath, [
    path.join(webRoot, "scripts/report-wasi-cost.mjs"),
    artifactPath,
    "--budget",
    path.join(webRoot, "benchmarks/latexd-wasi-cost-budget.json"),
    "--output",
    path.join(outputDir, "latexd-wasi-cost.json"),
    "--compile-samples",
    "0"
  ], { stdio: "inherit" });
  child.on("error", reject);
  child.on("exit", (code) => code === 0
    ? resolve()
    : reject(new Error(`WASI cost reporter exited with ${code}`)));
});

await cp(artifactPath, path.join(outputDir, "latexd-wasi.wasm"));
