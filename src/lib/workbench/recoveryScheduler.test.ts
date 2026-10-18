import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { RecoveryScheduler } from "./recoveryScheduler";

describe("RecoveryScheduler", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("writes 2 s after the last edit", () => {
    const flush = vi.fn();
    const s = new RecoveryScheduler(flush);
    s.touch("a.typ");
    vi.advanceTimersByTime(1999);
    expect(flush).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(flush).toHaveBeenCalledWith("a.typ");
    expect(s.isPending("a.typ")).toBe(false);
  });

  it("keeps waiting while edits keep coming, but never past 10 s", () => {
    const flush = vi.fn();
    const s = new RecoveryScheduler(flush);
    // An edit every 500 ms for 30 s: debounce alone would never fire.
    for (let t = 0; t < 30_000; t += 500) {
      s.touch("a.typ");
      vi.advanceTimersByTime(500);
    }
    // First flush at 10 s, then one every 10 s: three in 30 s.
    expect(flush.mock.calls.length).toBeGreaterThanOrEqual(2);
    expect(flush.mock.calls.length).toBeLessThanOrEqual(3);
  });

  it("tracks each document on its own", () => {
    const flush = vi.fn();
    const s = new RecoveryScheduler(flush);
    s.touch("a");
    vi.advanceTimersByTime(1000);
    s.touch("b");
    vi.advanceTimersByTime(1000);
    expect(flush.mock.calls).toEqual([["a"]]);
    vi.advanceTimersByTime(1000);
    expect(flush.mock.calls).toEqual([["a"], ["b"]]);
  });

  it("cancel (a save or a discard) stops the pending write", () => {
    const flush = vi.fn();
    const s = new RecoveryScheduler(flush);
    s.touch("a");
    s.cancel("a");
    vi.advanceTimersByTime(60_000);
    expect(flush).not.toHaveBeenCalled();
    s.cancel("never-touched");
  });

  it("flushNow writes at once and only once", () => {
    const flush = vi.fn();
    const s = new RecoveryScheduler(flush);
    s.touch("a");
    s.touch("b");
    s.flushNow("a");
    expect(flush.mock.calls).toEqual([["a"]]);
    s.flushAll();
    expect(flush.mock.calls).toEqual([["a"], ["b"]]);
    vi.advanceTimersByTime(60_000);
    expect(flush).toHaveBeenCalledTimes(2);
    s.flushNow("a");
    expect(flush).toHaveBeenCalledTimes(2);
  });

  it("carries a due snapshot to the new name when a file is renamed", () => {
    const flush = vi.fn();
    const s = new RecoveryScheduler(flush);
    s.touch("old.typ");
    s.rename("old.typ", "new.typ");
    vi.advanceTimersByTime(2000);
    expect(flush.mock.calls).toEqual([["new.typ"]]);
    s.rename("nothing", "else");
    expect(s.keys()).toEqual([]);
  });

  it("dispose drops everything", () => {
    const flush = vi.fn();
    const s = new RecoveryScheduler(flush);
    s.touch("a");
    s.touch("b");
    s.dispose();
    vi.advanceTimersByTime(60_000);
    expect(flush).not.toHaveBeenCalled();
  });
});
