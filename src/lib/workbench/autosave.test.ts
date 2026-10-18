import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  AUTOSAVE_DELAY_MS,
  AutosaveScheduler,
  autosaveMode,
  canAutosave,
} from "./autosave";

describe("autosave rules", () => {
  it("reads the setting, defaulting to after a delay", () => {
    expect(autosaveMode("off")).toBe("off");
    expect(autosaveMode("on-focus-loss")).toBe("on-focus-loss");
    expect(autosaveMode("after-delay")).toBe("after-delay");
    expect(autosaveMode(undefined)).toBe("after-delay");
    expect(autosaveMode("nonsense")).toBe("after-delay");
  });

  it("saves only a modified document that is in step with the disk", () => {
    const base = { dirty: true, external: null, comparing: false } as const;
    expect(canAutosave(base)).toBe(true);
    expect(canAutosave({ ...base, dirty: false })).toBe(false);
    expect(canAutosave({ ...base, external: "changed" })).toBe(false);
    expect(canAutosave({ ...base, external: "deleted" })).toBe(false);
    expect(canAutosave({ ...base, comparing: true })).toBe(false);
    expect(canAutosave(undefined)).toBe(false);
  });
});

describe("autosave scheduler", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("saves once typing pauses, not on every keystroke", () => {
    const run = vi.fn();
    const scheduler = new AutosaveScheduler(run);
    scheduler.touch("a.typ");
    vi.advanceTimersByTime(AUTOSAVE_DELAY_MS - 1);
    scheduler.touch("a.typ");
    vi.advanceTimersByTime(AUTOSAVE_DELAY_MS - 1);
    expect(run).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(run).toHaveBeenCalledTimes(1);
    expect(run).toHaveBeenCalledWith("a.typ");
  });

  it("keeps files apart and lets a save cancel the wait", () => {
    const run = vi.fn();
    const scheduler = new AutosaveScheduler(run);
    scheduler.touch("a.typ");
    scheduler.touch("b.tex");
    scheduler.cancel("a.typ");
    expect(scheduler.pending).toEqual(["b.tex"]);
    vi.advanceTimersByTime(AUTOSAVE_DELAY_MS);
    expect(run).toHaveBeenCalledTimes(1);
    expect(run).toHaveBeenCalledWith("b.tex");
  });

  it("cancelAll drops everything", () => {
    const run = vi.fn();
    const scheduler = new AutosaveScheduler(run);
    scheduler.touch("a.typ");
    scheduler.touch("b.tex");
    scheduler.cancelAll();
    vi.advanceTimersByTime(AUTOSAVE_DELAY_MS * 2);
    expect(run).not.toHaveBeenCalled();
    expect(scheduler.pending).toEqual([]);
  });
});
