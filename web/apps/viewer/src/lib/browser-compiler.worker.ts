import { runBrowserCompiler } from "./browser-compiler-runtime";
import type { BrowserCompileRequest, BrowserCompileResponse } from "./browser-compiler-protocol";

// Keep DOM and WebWorker global declarations separate without adding the
// conflicting webworker lib to the Svelte app's TypeScript compilation.
const scope = globalThis as unknown as {
  onmessage: ((event: MessageEvent<BrowserCompileRequest>) => void) | null;
  postMessage(message: BrowserCompileResponse, transfer?: Transferable[]): void;
};

scope.onmessage = async ({ data }) => {
  const { requestId, revision, files, entry, compilerUrl } = data;
  try {
    const result = await runBrowserCompiler(files, entry, revision, compilerUrl);
    scope.postMessage({ type: "compiled", requestId, revision, result }, [result.pdf.buffer as ArrayBuffer]);
  } catch (error) {
    scope.postMessage({
      type: "failed", requestId, revision,
      error: error instanceof Error ? error.message : String(error)
    });
  }
};
