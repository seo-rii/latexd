import { base } from "$app/paths";
import { BrowserCompilerClient } from "./browser-compiler-client";

export type {
  BrowserBuildMetadata,
  BrowserFontAsset,
  BrowserPageDisplayList,
  BrowserPagesArtifact
} from "./browser-artifacts";
export type { BrowserCompileResult } from "./browser-compiler-protocol";

// Construction is SSR-safe: the Worker is created only on the first request.
export function createBrowserCompiler() {
  return new BrowserCompilerClient(
    () => new Worker(new URL("./browser-compiler.worker.ts", import.meta.url), {
      type: "module",
      name: "latexd-compiler"
    }),
    `${base}/wasi/latexd-wasi.wasm`
  );
}

const defaultCompiler = createBrowserCompiler();

export function compileProjectInBrowser(
  files: Record<string, Uint8Array>,
  entry = "main.tex",
  revision = 0,
  options: { signal?: AbortSignal } = {}
) {
  return defaultCompiler.compileProject(files, entry, revision, options);
}

export function compileInBrowser(source: string, revision = 0) {
  return defaultCompiler.compileSource(source, revision);
}
