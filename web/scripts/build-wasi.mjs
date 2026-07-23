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
await cp(
  path.join(repoRoot, "target/wasm32-wasip1/release/latexd-wasi.wasm"),
  path.join(outputDir, "latexd-wasi.wasm")
);
