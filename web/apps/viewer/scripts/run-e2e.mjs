import { spawnSync } from "node:child_process";

// Windows resolves pnpm to a .cmd shim, and Node refuses to spawn .cmd/.bat files
// without a shell (EINVAL since the CVE-2024-27980 fix). Each step therefore runs
// as one shell command; the commands are fixed literals, so nothing needs quoting.
const wasm = process.argv.includes("--wasm");
const env = {
  ...process.env,
  ...(wasm
    ? { LATEXD_VIEWER_BASE_PATH: "/latexd", VITE_LATEXD_BROWSER_ONLY: "true" }
    : { LATEXD_VIEWER_BASE_PATH: "/viewer" })
};
const playwright = wasm
  ? "pnpm exec playwright test --config playwright.wasm.config.ts"
  : "pnpm exec playwright test";

for (const command of ["pnpm build", playwright]) {
  const result = spawnSync(command, {
    env,
    shell: true,
    stdio: "inherit"
  });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}
