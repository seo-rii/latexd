import { readFile, writeFile } from "node:fs/promises";

import {
  assertWasmCostWithinBudget,
  measureWasmCost
} from "./wasm-cost.mjs";

const [artifactPath, ...options] = process.argv.slice(2);
if (!artifactPath) {
  throw new Error(
    "usage: report-wasi-cost.mjs <artifact> --budget <path> --output <path> [--compile-samples <count>]"
  );
}

let budgetPath;
let outputPath;
let compileSamples = 3;
for (let index = 0; index < options.length; index += 2) {
  const option = options[index];
  const value = options[index + 1];
  if (value === undefined) {
    throw new Error(`missing value for ${option}`);
  }
  if (option === "--budget") {
    budgetPath = value;
  } else if (option === "--output") {
    outputPath = value;
  } else if (option === "--compile-samples") {
    compileSamples = Number(value);
  } else {
    throw new Error(`unknown option: ${option}`);
  }
}

if (!budgetPath || !outputPath) {
  throw new Error("--budget and --output are required");
}

const budget = JSON.parse(await readFile(budgetPath, "utf8"));
const report = await measureWasmCost(artifactPath, { compileSamples });
const serializedReport = `${JSON.stringify(report, null, 2)}\n`;
await writeFile(outputPath, serializedReport);
process.stdout.write(serializedReport);
assertWasmCostWithinBudget(report, budget);
