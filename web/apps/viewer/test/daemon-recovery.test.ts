import assert from "node:assert/strict";
import test from "node:test";

import { watchForDaemon } from "../src/lib/daemon-recovery.ts";

function manualScheduler() {
  const timers = new Map<number, { callback: () => void; delayMs: number }>();
  let nextId = 1;
  return {
    timers,
    schedule(callback: () => void, delayMs: number) {
      const id = nextId++;
      timers.set(id, { callback, delayMs });
      return id;
    },
    cancel(id: number) {
      timers.delete(id);
    },
    fire() {
      const [id, timer] = [...timers.entries()][0];
      timers.delete(id);
      timer.callback();
      return timer.delayMs;
    }
  };
}

const settle = () => new Promise((resolve) => setImmediate(resolve));

test("probes with backoff until the daemon answers, then reports it once", async () => {
  const scheduler = manualScheduler();
  const outcomes = [false, false, false, true];
  let probes = 0;
  let available = 0;
  watchForDaemon({
    probe: async () => {
      probes += 1;
      if (!outcomes.shift()) {
        throw new Error("daemon still starting");
      }
    },
    onAvailable: () => {
      available += 1;
    },
    initialDelayMs: 1000,
    maxDelayMs: 3000,
    schedule: scheduler.schedule,
    cancel: scheduler.cancel
  });

  const delays: number[] = [];
  while (scheduler.timers.size > 0) {
    delays.push(scheduler.fire());
    await settle();
  }

  assert.deepEqual(delays, [1000, 2000, 3000, 3000]);
  assert.equal(probes, 4);
  assert.equal(available, 1);
});

test("stopping the watch cancels the pending probe and ignores an in-flight answer", async () => {
  const scheduler = manualScheduler();
  let finishProbe: () => void = () => {};
  let available = 0;
  const stop = watchForDaemon({
    probe: () => new Promise<void>((resolve) => {
      finishProbe = resolve;
    }),
    onAvailable: () => {
      available += 1;
    },
    schedule: scheduler.schedule,
    cancel: scheduler.cancel
  });

  scheduler.fire();
  stop();
  finishProbe();
  await settle();
  assert.equal(available, 0);
  assert.equal(scheduler.timers.size, 0);

  const stopBeforeProbe = watchForDaemon({
    probe: async () => {},
    onAvailable: () => {
      available += 1;
    },
    schedule: scheduler.schedule,
    cancel: scheduler.cancel
  });
  stopBeforeProbe();
  assert.equal(scheduler.timers.size, 0);
  assert.equal(available, 0);
});
