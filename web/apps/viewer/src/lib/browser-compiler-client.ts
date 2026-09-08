import type {
  BrowserCompileRequest,
  BrowserCompileResponse,
  BrowserCompileResult
} from "./browser-compiler-protocol";

export interface BrowserCompilerWorker {
  onmessage: ((event: MessageEvent<BrowserCompileResponse>) => void) | null;
  onerror: ((event: ErrorEvent) => void) | null;
  onmessageerror: ((event: MessageEvent) => void) | null;
  postMessage(message: BrowserCompileRequest, transfer: Transferable[]): void;
  terminate(): void;
}

type PendingCompile = {
  requestId: number;
  revision: number;
  resolve: (result: BrowserCompileResult) => void;
  reject: (error: Error) => void;
  cleanup: () => void;
};

/** Owns one cancellable worker, not a persistent TeX VM or checkpoint session. */
export class BrowserCompilerClient {
  private worker: BrowserCompilerWorker | null = null;
  private pending: PendingCompile | null = null;
  private serial = 0;
  private disposed = false;

  constructor(
    private readonly createWorker: () => BrowserCompilerWorker,
    private readonly compilerUrl: string,
    private readonly timeoutMs = 120_000
  ) {}

  compileSource(source: string, revision = 0, options: { signal?: AbortSignal } = {}) {
    return this.compileProject(
      { "main.tex": new TextEncoder().encode(source) }, "main.tex", revision, options
    );
  }

  compileProject(
    files: Record<string, Uint8Array>,
    entry = "main.tex",
    revision = 0,
    { signal }: { signal?: AbortSignal } = {}
  ): Promise<BrowserCompileResult> {
    if (this.disposed) {
      return Promise.reject(new Error("Browser compiler is disposed"));
    }
    if (signal?.aborted) {
      return Promise.reject(new DOMException("Browser compilation cancelled", "AbortError"));
    }
    if (!Number.isSafeInteger(revision) || revision < 0) {
      return Promise.reject(new Error("Browser compile revision must be a non-negative integer"));
    }
    // WASI _start is synchronous. A cancel message cannot interrupt it; only
    // terminating the worker can stop an obsolete compile promptly.
    this.cancel();
    const requestId = ++this.serial;
    return new Promise((resolve, reject) => {
      let timer: ReturnType<typeof setTimeout> | undefined;
      const abort = () => this.cancel();
      this.pending = {
        requestId,
        revision,
        resolve,
        reject,
        cleanup: () => {
          clearTimeout(timer);
          signal?.removeEventListener("abort", abort);
        }
      };
      signal?.addEventListener("abort", abort, { once: true });
      timer = setTimeout(() => {
        this.fail(new Error(`Browser compilation timed out after ${this.timeoutMs} ms`));
      }, this.timeoutMs);
      try {
        if (!this.worker) {
          const worker = this.createWorker();
          this.worker = worker;
          worker.onmessage = ({ data }) => {
            const pending = this.pending;
            // Both the worker generation and request identity must match. A
            // queued response from a terminated worker cannot publish a build.
            if (worker !== this.worker || !pending || data.requestId !== pending.requestId) {
              return;
            }
            if (data.revision !== pending.revision) {
              this.fail(new Error("Browser compiler returned a different revision"));
              return;
            }
            if (data.type === "failed") {
              this.fail(new Error(data.error));
              return;
            }
            if (data.type !== "compiled" || data.result?.build_metadata?.revision !== pending.revision) {
              this.fail(new Error("Browser compiler returned invalid build metadata"));
              return;
            }
            this.pending = null;
            pending.cleanup();
            pending.resolve(data.result);
          };
          worker.onerror = (event) => {
            event.preventDefault();
            if (worker === this.worker) {
              this.fail(new Error(event.message || "Browser compiler worker crashed"));
            }
          };
          worker.onmessageerror = () => {
            if (worker === this.worker) {
              this.fail(new Error("Browser compiler response could not be decoded"));
            }
          };
        }
        // Transfer dedicated copies: the editor and last-good asset snapshot
        // retain their original buffers even while compilation is cancelled.
        const snapshot = Object.fromEntries(
          Object.entries(files).map(([path, bytes]) => [path, bytes.slice()])
        );
        this.worker.postMessage({
          type: "compile", requestId, revision, entry, files: snapshot,
          compilerUrl: this.compilerUrl
        }, Object.values(snapshot).map((bytes) => bytes.buffer));
      } catch (error) {
        this.fail(error instanceof Error ? error : new Error(String(error)));
      }
    });
  }

  cancel() {
    if (this.pending) {
      this.fail(new DOMException("Browser compilation cancelled", "AbortError"));
    }
  }

  dispose() {
    this.disposed = true;
    this.cancel();
    this.stopWorker();
  }

  private fail(error: Error) {
    const pending = this.pending;
    this.pending = null;
    this.stopWorker();
    pending?.cleanup();
    pending?.reject(error);
  }

  private stopWorker() {
    const worker = this.worker;
    this.worker = null;
    if (worker) {
      worker.onmessage = null;
      worker.onerror = null;
      worker.onmessageerror = null;
      worker.terminate();
    }
  }
}
