import path from "node:path";
import { fileURLToPath } from "node:url";

import { defineConfig } from "@playwright/test";

const viewerRoot = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(viewerRoot, "../../..");

export default defineConfig({
  testDir: "./test",
  testMatch: "browser-mode.spec.ts",
  timeout: 30_000,
  workers: 1,
  use: {
    baseURL: "http://127.0.0.1:4390/latexd/",
    browserName: "chromium",
    headless: true,
    trace: "on-first-retry"
  },
  webServer: {
    command: "LATEXD_VIEWER_BASE_PATH=/latexd VITE_LATEXD_BROWSER_ONLY=true pnpm -C web --filter @latexd/viewer-app exec vite preview --host 127.0.0.1 --port 4390",
    cwd: repoRoot,
    url: "http://127.0.0.1:4390/latexd/",
    reuseExistingServer: !process.env.CI,
    timeout: 30_000
  }
});
