export interface DaemonWatchOptions {
  /** Resolves once the daemon answers; rejects while it is unreachable. */
  probe(): Promise<unknown>;
  onAvailable(): void;
  initialDelayMs?: number;
  maxDelayMs?: number;
  schedule?: (callback: () => void, delayMs: number) => unknown;
  cancel?: (handle: any) => void;
}

// The viewer falls back to the browser compiler when the daemon does not answer quickly,
// for example while `cargo run` is still compiling it under `pnpm dev`. Keep probing with
// backoff so daemon mode can take over once the daemon is up. Returns a stop function.
export function watchForDaemon({
  probe,
  onAvailable,
  initialDelayMs = 1000,
  maxDelayMs = 10_000,
  schedule = (callback, delayMs) => setTimeout(callback, delayMs),
  cancel = (handle) => clearTimeout(handle)
}: DaemonWatchOptions) {
  let stopped = false;
  let handle: unknown = null;

  const attemptAfter = (delayMs: number) => {
    handle = schedule(() => {
      handle = null;
      probe().then(
        () => {
          if (!stopped) {
            stopped = true;
            onAvailable();
          }
        },
        () => {
          if (!stopped) {
            attemptAfter(Math.min(delayMs * 2, maxDelayMs));
          }
        }
      );
    }, delayMs);
  };
  attemptAfter(initialDelayMs);

  return () => {
    stopped = true;
    if (handle !== null) {
      cancel(handle);
      handle = null;
    }
  };
}
