import { afterEach, describe, expect, it, vi } from "vitest";

import { noticeLine, Throttled, treeSummary } from "./liveNotices";

const names = (peer: string): string =>
  ({ "1": "Ada", "2": "Grace" })[peer] ?? "Someone";

afterEach(() => vi.useRealTimers());

describe("Throttled", () => {
  it("hands over a burst once", () => {
    vi.useFakeTimers();
    const out: number[][] = [];
    const t = new Throttled<number>((items) => out.push([...items]), 100);
    t.push(1);
    t.push(2);
    vi.advanceTimersByTime(99);
    expect(out).toEqual([]);
    t.push(3);
    vi.advanceTimersByTime(2);
    expect(out).toEqual([[1, 2, 3]]);
    t.push(4);
    vi.advanceTimersByTime(100);
    expect(out).toEqual([[1, 2, 3], [4]]);
  });

  it("drops what is waiting on cancel", () => {
    vi.useFakeTimers();
    const out: number[][] = [];
    const t = new Throttled<number>((items) => out.push([...items]), 100);
    t.push(1);
    t.cancel();
    vi.advanceTimersByTime(500);
    expect(out).toEqual([]);
  });
});

describe("sentences", () => {
  it("names one change and counts many", () => {
    expect(
      treeSummary([{ op: "delete", path: "src/a.typ", by: "1" }], names)?.text,
    ).toBe("Ada deleted a.typ.");
    expect(
      treeSummary(
        [
          { op: "create", path: "a", by: "1" },
          { op: "create", path: "b", by: "1" },
        ],
        names,
      )?.text,
    ).toBe("Ada changed 2 files.");
    expect(
      treeSummary(
        [
          { op: "create", path: "a", by: "1" },
          { op: "delete", path: "b", by: "2" },
        ],
        names,
      )?.text,
    ).toBe("2 people changed 2 files.");
    expect(treeSummary([], names)).toBeNull();
  });

  it("turns notices into lines", () => {
    expect(noticeLine({ session: "s", kind: "joined", peer: "2" }, names).text).toBe(
      "Grace joined.",
    );
    expect(
      noticeLine({ session: "s", kind: "deleted", peer: "1", path: "x/y.typ" }, names),
    ).toEqual({ kind: "deleted", text: "Ada deleted y.typ.", path: "x/y.typ" });
  });
});
