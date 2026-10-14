type Waiter<Result> = {
  resolve: (result: Result) => void;
  reject: (error: unknown) => void;
};

type FileSaves<Result> = {
  inFlight: { content: string; waiters: Waiter<Result>[] };
  next: { content: string; waiters: Waiter<Result>[] } | null;
};

// Writes to one file are sent one at a time. While a write is in flight, newer content
// waits, and only the newest waiting content is written next. Two writes to the same
// file therefore never race on the daemon's atomic rename, so the file ends up with the
// newest content that was saved, never an older one that happened to finish last.
//
// `save` settles with the result of the write that persisted its content or newer
// content, and rejects when that write fails.
export function createSourceSaveQueue<Result>(
  write: (file: string, content: string) => Promise<Result>
) {
  const files = new Map<string, FileSaves<Result>>();

  const start = (file: string, saves: FileSaves<Result>) => {
    const { content, waiters } = saves.inFlight;
    // The executor starts the write now and turns a synchronous throw into a rejection.
    void new Promise<Result>((resolve) => resolve(write(file, content)))
      .then(
        (result) => waiters.forEach((waiter) => waiter.resolve(result)),
        (error) => waiters.forEach((waiter) => waiter.reject(error))
      )
      .finally(() => {
        if (saves.next) {
          saves.inFlight = saves.next;
          saves.next = null;
          start(file, saves);
        } else {
          files.delete(file);
        }
      });
  };

  return {
    save(file: string, content: string): Promise<Result> {
      return new Promise<Result>((resolve, reject) => {
        const waiter = { resolve, reject };
        const saves = files.get(file);
        if (!saves) {
          const created = { inFlight: { content, waiters: [waiter] }, next: null };
          files.set(file, created);
          start(file, created);
        } else if (!saves.next && saves.inFlight.content === content) {
          saves.inFlight.waiters.push(waiter);
        } else if (saves.next) {
          saves.next.content = content;
          saves.next.waiters.push(waiter);
        } else {
          saves.next = { content, waiters: [waiter] };
        }
      });
    }
  };
}
