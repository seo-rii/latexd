import path from "node:path";
import { fileURLToPath } from "node:url";

import { defineConfig } from "@playwright/test";
import { normalizeBasePath } from "./base-path.mjs";

const viewerRoot = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(viewerRoot, "../../..");
const viewerBasePath = normalizeBasePath(process.env.LATEXD_VIEWER_BASE_PATH);
const serverOrigin = "http://127.0.0.1:4382";
const baseURL = `${serverOrigin}${viewerBasePath || ""}/`;

export default defineConfig({
  testDir: "./test",
  testMatch: "*.spec.ts",
  testIgnore: "browser-mode.spec.ts",
  timeout: 30_000,
  workers: 1,
  use: {
    baseURL,
    browserName: "chromium",
    headless: true,
    trace: "on-first-retry"
  },
  webServer: {
    command: "node web/apps/viewer/scripts/run-e2e-server.mjs",
    cwd: repoRoot,
    env: {
      ...process.env,
      LATEXD_VIEWER_BASE_PATH: viewerBasePath
    },
    url: `${serverOrigin}${viewerBasePath || ""}/api/state`,
    reuseExistingServer: !process.env.CI,
    timeout: 120_000
  }
});
