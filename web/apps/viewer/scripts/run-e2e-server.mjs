import { mkdtemp, cp, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "../../../..");
const sourceFixtureRoot = path.join(repoRoot, "fixtures", "arxiv-basic");
const tempRoot = await mkdtemp(path.join(os.tmpdir(), "latexd-viewer-e2e-"));
const copiedFixtureRoot = path.join(tempRoot, "arxiv-basic");

await cp(sourceFixtureRoot, copiedFixtureRoot, { recursive: true });

const child = spawn(
  "cargo",
  [
    "run",
    "-q",
    "-p",
    "latexd",
    "--",
    "serve",
    "--root",
    copiedFixtureRoot,
    "--compiler-bin",
    "internal",
    "--bind",
    "127.0.0.1:4382"
  ],
  {
    cwd: repoRoot,
    env: process.env,
    stdio: "inherit"
  }
);

let cleaned = false;

const cleanup = async () => {
  if (cleaned) {
    return;
  }
  cleaned = true;
  await rm(tempRoot, { recursive: true, force: true });
};

const forwardSignal = (signal) => {
  child.kill(signal);
};

process.on("SIGINT", forwardSignal);
process.on("SIGTERM", forwardSignal);

child.on("exit", async (code, signal) => {
  process.off("SIGINT", forwardSignal);
  process.off("SIGTERM", forwardSignal);
  await cleanup();
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 0);
});

child.on("error", async (error) => {
  console.error(error);
  await cleanup();
  process.exit(1);
});
