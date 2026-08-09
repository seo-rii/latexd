import { cp, mkdir } from "node:fs/promises";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const webRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = path.resolve(webRoot, "..");

await new Promise((resolve, reject) => {
  const child = spawn("cargo", [
    "build",
    "--manifest-path",
    path.join(repoRoot, "Cargo.toml"),
    "--package",
    "latexd-wasi",
    "--target",
    "wasm32-wasip1",
    "--release"
  ], { stdio: "inherit" });
  child.on("error", reject);
  child.on("exit", (code) => code === 0 ? resolve() : reject(new Error(`cargo exited with ${code}`)));
});

const outputDir = path.join(webRoot, "apps/viewer/static/wasi");
await mkdir(outputDir, { recursive: true });
const artifactPath = path.join(
  repoRoot,
  "target/wasm32-wasip1/release/latexd-wasi.wasm"
);

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
