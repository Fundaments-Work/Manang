import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { IdleRunner } from "./idle";

describe("IdleRunner", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("runs once after a burst of pokes, 400 ms after the last", async () => {
    const task = vi.fn(() => Promise.resolve());
    const runner = new IdleRunner(task);
    for (let i = 0; i < 10; i += 1) {
      runner.poke();
      await vi.advanceTimersByTimeAsync(100);
    }
    expect(task).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(400);
    expect(task).toHaveBeenCalledTimes(1);
  });

  it("never starts runs closer together than the minimum gap", async () => {
    const task = vi.fn(() => Promise.resolve());
    const runner = new IdleRunner(task, 10, 1000);
    runner.poke();
    await vi.advanceTimersByTimeAsync(20);
    expect(task).toHaveBeenCalledTimes(1);
    runner.poke();
    await vi.advanceTimersByTimeAsync(500);
    expect(task).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(600);
    expect(task).toHaveBeenCalledTimes(2);
  });

  it("caps a poke-per-millisecond stream at the configured rate", async () => {
    const task = vi.fn(() => Promise.resolve());
    const runner = new IdleRunner(task, 0, 250);
    for (let ms = 0; ms < 3000; ms += 1) {
      runner.poke();
      await vi.advanceTimersByTimeAsync(1);
    }
    // 3 seconds: the first run at once, then at most four per second
    expect(task.mock.calls.length).toBeLessThanOrEqual(13);
    expect(task.mock.calls.length).toBeGreaterThan(0);
  });

  it("does not overlap runs and schedules exactly one follow-up", async () => {
    let active = 0;
    let peak = 0;
    const task = vi.fn(async () => {
      active += 1;
      peak = Math.max(peak, active);
      await new Promise((resolve) => setTimeout(resolve, 300));
      active -= 1;
    });
    const runner = new IdleRunner(task, 10, 0);
    runner.poke();
    await vi.advanceTimersByTimeAsync(20);
    runner.poke();
    runner.poke();
    await vi.advanceTimersByTimeAsync(1000);
    expect(peak).toBe(1);
    expect(task).toHaveBeenCalledTimes(2);
  });

  it("survives a failing task and stops after destroy", async () => {
    const task = vi.fn(() => Promise.reject(new Error("boom")));
    const runner = new IdleRunner(task, 10, 0);
    runner.poke();
    await vi.advanceTimersByTimeAsync(20);
    runner.poke();
    await vi.advanceTimersByTimeAsync(20);
    expect(task).toHaveBeenCalledTimes(2);
    runner.destroy();
    runner.poke();
    await vi.advanceTimersByTimeAsync(1000);
    expect(task).toHaveBeenCalledTimes(2);
  });
});
