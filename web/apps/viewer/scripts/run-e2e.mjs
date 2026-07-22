import { spawnSync } from "node:child_process";

const command = process.platform === "win32" ? "pnpm.cmd" : "pnpm";
const env = {
  ...process.env,
  LATEXD_VIEWER_BASE_PATH: "/viewer"
};

for (const args of [["build"], ["exec", "playwright", "test"]]) {
  const result = spawnSync(command, args, {
    env,
    stdio: "inherit"
  });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}
