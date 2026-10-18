import { describe, expect, it } from "vitest";

import {
  RECENT_KEY,
  RECENT_LIMIT,
  hexToHsl,
  hslToHex,
  loadRecent,
  parseHexInput,
  pushRecent,
  saveRecent,
} from "./colour";

class MemoryStorage {
  readonly #data = new Map<string, string>();
  getItem(key: string): string | null {
    return this.#data.get(key) ?? null;
  }
  setItem(key: string, value: string): void {
    this.#data.set(key, value);
  }
}

describe("hex and HSL", () => {
  it("round-trips colours through HSL", () => {
    for (const hex of [
      "#c6f135",
      "#0072b2",
      "#ff8800",
      "#808080",
      "#000000",
      "#ffffff",
    ]) {
      const hsl = hexToHsl(hex);
      expect(hsl, hex).toBeDefined();
      expect(hslToHex(hsl as { h: number; s: number; l: number }), hex).toBe(hex);
    }
  });

  it("knows the primaries", () => {
    expect(hslToHex({ h: 0, s: 100, l: 50 })).toBe("#ff0000");
    expect(hslToHex({ h: 120, s: 100, l: 50 })).toBe("#00ff00");
    expect(hslToHex({ h: 240, s: 100, l: 50 })).toBe("#0000ff");
    expect(hexToHsl("#00ff00")).toEqual({ h: 120, s: 100, l: 50 });
    expect(hexToHsl("#808080")?.s).toBe(0);
  });

  it("clamps out-of-range input and reads only hex", () => {
    expect(hslToHex({ h: 400, s: 150, l: -5 })).toBe("#000000");
    expect(hslToHex({ h: Number.NaN, s: 50, l: 50 })).toMatch(/^#[0-9a-f]{6}$/);
    expect(hexToHsl("ink")).toBeUndefined();
    expect(hexToHsl("#12")).toBeUndefined();
  });

  it("accepts what people type in the hex field", () => {
    expect(parseHexInput("ff8800")).toBe("#ff8800");
    expect(parseHexInput(" #FF8800 ")).toBe("#ff8800");
    expect(parseHexInput("#f80")).toBe("#ff8800");
    expect(parseHexInput("#ff88")).toBeUndefined();
    expect(parseHexInput("nothex")).toBeUndefined();
    expect(parseHexInput("")).toBeUndefined();
  });
});

describe("recent colours", () => {
  it("puts the newest first, without duplicates, at most eight", () => {
    let recent: string[] = [];
    for (let i = 0; i < 12; i += 1) {
      recent = pushRecent(recent, hslToHex({ h: i * 25, s: 80, l: 50 }));
    }
    expect(recent).toHaveLength(RECENT_LIMIT);
    expect(recent[0]).toBe(hslToHex({ h: 275, s: 80, l: 50 }));
    const again = pushRecent(recent, (recent[3] as string).toUpperCase());
    expect(again[0]).toBe(recent[3]);
    expect(again).toHaveLength(RECENT_LIMIT);
    expect(new Set(again).size).toBe(again.length);
  });

  it("ignores values that are not hex colours", () => {
    expect(pushRecent(["#112233"], "ink")).toEqual(["#112233"]);
  });

  it("persists per user and survives bad storage", () => {
    const storage = new MemoryStorage();
    saveRecent(["#112233", "#445566"], storage);
    expect(loadRecent(storage)).toEqual(["#112233", "#445566"]);
    storage.setItem(RECENT_KEY, "{not json");
    expect(loadRecent(storage)).toEqual([]);
    storage.setItem(RECENT_KEY, JSON.stringify(["#abc", 3, "ink", "#ZZZZZZ"]));
    expect(loadRecent(storage)).toEqual(["#aabbcc"]);
    const blocked = {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("blocked");
      },
    };
    expect(loadRecent(blocked)).toEqual([]);
    expect(() => saveRecent(["#112233"], blocked)).not.toThrow();
    expect(loadRecent(undefined)).toEqual([]);
  });
});
