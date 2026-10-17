import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { createScheduler } from "./scheduler";

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe("createScheduler", () => {
  it("collapses a burst of requests into one run after the delay", async () => {
    const run = vi.fn(async () => {});
    const s = createScheduler(run, 250);
    s.request();
    await vi.advanceTimersByTimeAsync(200);
    s.request();
    await vi.advanceTimersByTimeAsync(200);
    expect(run).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(60);
    expect(run).toHaveBeenCalledTimes(1);
  });

  it("never runs two at once and runs once more for requests made meanwhile", async () => {
    let release: () => void = () => {};
    let active = 0;
    let peak = 0;
    const run = vi.fn(async () => {
      active += 1;
      peak = Math.max(peak, active);
      await new Promise<void>((resolve) => (release = resolve));
      active -= 1;
    });
    const s = createScheduler(run, 10);
    s.flush();
    expect(s.running).toBe(true);
    s.request(0);
    s.request(0);
    await vi.advanceTimersByTimeAsync(5);
    expect(run).toHaveBeenCalledTimes(1);
    release();
    await vi.advanceTimersByTimeAsync(0);
    expect(run).toHaveBeenCalledTimes(2);
    release();
    await vi.advanceTimersByTimeAsync(0);
    expect(run).toHaveBeenCalledTimes(2);
    expect(peak).toBe(1);
  });

  it("cancel drops a pending run, and a failing job does not stop the scheduler", async () => {
    const run = vi.fn(async () => {
      throw new Error("boom");
    });
    const s = createScheduler(run, 50);
    s.request();
    s.cancel();
    await vi.advanceTimersByTimeAsync(100);
    expect(run).not.toHaveBeenCalled();
    s.flush();
    await vi.advanceTimersByTimeAsync(0);
    s.flush();
    await vi.advanceTimersByTimeAsync(0);
    expect(run).toHaveBeenCalledTimes(2);
    expect(s.running).toBe(false);
  });
});
