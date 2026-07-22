import { readdir, rm } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const webRoot = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const workspaceGroups = ["apps", "packages"];
const generatedNames = ["build", "dist", ".svelte-kit"];

for (const group of workspaceGroups) {
  const groupRoot = path.join(webRoot, group);
  let entries = [];
  try {
    entries = await readdir(groupRoot, { withFileTypes: true });
  } catch (error) {
    if (error?.code !== "ENOENT") {
      throw error;
    }
  }
  for (const entry of entries) {
    if (!entry.isDirectory()) {
      continue;
    }
    for (const generatedName of generatedNames) {
      await rm(path.join(groupRoot, entry.name, generatedName), {
        force: true,
        recursive: true
      });
    }
    await rm(path.join(groupRoot, entry.name, "node_modules", ".vite-temp"), {
      force: true,
      recursive: true
    });
  }
}
