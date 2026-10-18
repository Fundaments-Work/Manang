import { describe, expect, it } from "vitest";

import { normalizePlan, parseVisualError, planFits, visualApi } from "./client";
import { NO_ATTR, SPAN_KIND } from "./protocol";
import { decodeSpanTable, encodeSpanTable } from "./table";

const SAMPLE = encodeSpanTable({
  revision: 7,
  from: 0,
  to: 30,
  docLen: 30,
  spans: [
    { start: 0, end: 10, kind: SPAN_KIND.heading, layer: 0, aux: 2 },
    { start: 0, end: 3, kind: SPAN_KIND.marker, layer: 1 },
    { start: 12, end: 20, kind: SPAN_KIND.link, layer: 0, attr: "https://é.example" },
    { start: 14, end: 15, kind: SPAN_KIND.marker, layer: 1 },
    { start: 22, end: 30, kind: SPAN_KIND.codeBlock, layer: 0, attr: "ts" },
  ],
});

describe("decodeSpanTable", () => {
  it("round-trips a table", () => {
    const result = decodeSpanTable(SAMPLE);
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    const { header, spans } = result.table;
    expect(header).toMatchObject({ revision: 7, docLen: 30, spanCount: 5, version: 1 });
    expect(header.truncated).toBe(false);
    expect(spans[0]).toEqual({
      start: 0,
      end: 10,
      kind: SPAN_KIND.heading,
      layer: 0,
      aux: 2,
      attr: null,
    });
    expect(spans[2]?.attr).toBe("https://é.example");
    expect(spans[4]?.attr).toBe("ts");
  });

  it("reads little-endian fields and the truncated flag", () => {
    const bytes = encodeSpanTable({
      revision: 2 ** 33 + 5,
      from: 1,
      to: 2,
      docLen: 4,
      truncated: true,
      spans: [{ start: 1, end: 2, kind: 1025, layer: 3, aux: 255, attr: null }],
    });
    expect([...bytes.subarray(0, 4)]).toEqual([0x4d, 0x56, 0x53, 0x31]);
    const result = decodeSpanTable(bytes);
    expect(result.ok && result.table.header.truncated).toBe(true);
    expect(result.ok && result.table.header.revision).toBe(2 ** 33 + 5);
    expect(result.ok && result.table.spans[0]).toMatchObject({
      kind: 1025,
      layer: 3,
      aux: 255,
    });
  });

  it("decodes from an unaligned view", () => {
    const padded = new Uint8Array(SAMPLE.length + 3);
    padded.set(SAMPLE, 3);
    expect(decodeSpanTable(padded.subarray(3)).ok).toBe(true);
  });

  it("accepts an empty table", () => {
    const empty = encodeSpanTable({
      revision: 0,
      from: 0,
      to: 0,
      docLen: 0,
      spans: [],
    });
    const result = decodeSpanTable(empty);
    expect(result.ok && result.table.spans.length).toBe(0);
  });

  it("refuses bad magic, version and a short header, without throwing", () => {
    const badMagic = SAMPLE.slice();
    badMagic[0] = 0;
    expect(decodeSpanTable(badMagic)).toEqual({ ok: false, reason: "bad magic" });
    const v2 = SAMPLE.slice();
    new DataView(v2.buffer).setUint16(4, 2, true);
    expect(decodeSpanTable(v2).ok).toBe(false);
    expect(decodeSpanTable(new Uint8Array(0)).ok).toBe(false);
    expect(decodeSpanTable(SAMPLE.subarray(0, 39)).ok).toBe(false);
  });

  it("refuses every truncation and trailing bytes", () => {
    for (let n = 0; n < SAMPLE.length; n += 1) {
      expect(decodeSpanTable(SAMPLE.subarray(0, n)).ok, `length ${n}`).toBe(false);
    }
    const longer = new Uint8Array(SAMPLE.length + 1);
    longer.set(SAMPLE);
    expect(decodeSpanTable(longer).ok).toBe(false);
  });

  it("refuses counts that do not fit the buffer", () => {
    const huge = SAMPLE.slice();
    new DataView(huge.buffer).setUint32(28, 0xffff_ffff, true);
    expect(decodeSpanTable(huge).ok).toBe(false);
    const strings = SAMPLE.slice();
    new DataView(strings.buffer).setUint32(32, 0xffff_ffff, true);
    expect(decodeSpanTable(strings).ok).toBe(false);
  });

  function withRecord(index: number, word: number, value: number): Uint8Array {
    const copy = SAMPLE.slice();
    new DataView(copy.buffer).setUint32(40 + index * 16 + word * 4, value, true);
    return copy;
  }

  it("refuses spans outside the document or empty", () => {
    expect(decodeSpanTable(withRecord(4, 1, 31)).ok).toBe(false);
    expect(decodeSpanTable(withRecord(0, 1, 0)).ok).toBe(false);
  });

  it("refuses unsorted and crossing spans", () => {
    expect(decodeSpanTable(withRecord(2, 0, 0)).ok).toBe(false);
    // Span 1 (0..3) stretched past its parent (0..10).
    expect(decodeSpanTable(withRecord(1, 1, 11)).ok).toBe(false);
  });

  it("refuses an attribute index outside the string table", () => {
    expect(decodeSpanTable(withRecord(0, 3, 99)).ok).toBe(false);
    expect(decodeSpanTable(withRecord(0, 3, NO_ATTR)).ok).toBe(true);
  });

  it("refuses string offsets that go backwards or past the bytes", () => {
    const base = 40 + 5 * 16;
    const back = SAMPLE.slice();
    new DataView(back.buffer).setUint32(base, 9999, true);
    expect(decodeSpanTable(back).ok).toBe(false);
  });

  it("never throws on random bytes", () => {
    let seed = 12345;
    const next = (): number => {
      seed = (seed * 1103515245 + 12345) & 0x7fffffff;
      return seed;
    };
    for (let i = 0; i < 500; i += 1) {
      const bytes = new Uint8Array(next() % 200);
      for (let j = 0; j < bytes.length; j += 1) bytes[j] = next() & 0xff;
      // Make some of them look like real tables with a corrupted body.
      if (i % 2 === 0 && bytes.length > 4) bytes.set([0x4d, 0x56, 0x53, 0x31]);
      expect(() => decodeSpanTable(bytes)).not.toThrow();
    }
    for (let i = 0; i < SAMPLE.length; i += 1) {
      const flipped = SAMPLE.slice();
      flipped[i] = (flipped[i] ?? 0) ^ 0xff;
      expect(() => decodeSpanTable(flipped)).not.toThrow();
    }
  });
});

describe("visualApi", () => {
  it("decodes blocks and passes plain arguments", async () => {
    const calls: unknown[] = [];
    const api = visualApi({
      run: () => Promise.reject(new Error("unused")),
      runBinary: (command, args) => {
        calls.push([command, args]);
        return Promise.resolve(SAMPLE);
      },
    });
    const result = await api.blocks({ session: 3, from: 0, to: 30 });
    expect(result.ok).toBe(true);
    expect(calls).toEqual([["visual.blocks", { session: 3, from: 0, to: 30 }]]);
  });

  it("turns a malformed table into a value, not a throw", async () => {
    const api = visualApi({
      run: () => Promise.reject(new Error("unused")),
      runBinary: () => Promise.resolve(new Uint8Array([1, 2, 3])),
    });
    expect((await api.blocks({ session: 1, from: 0, to: 1 })).ok).toBe(false);
  });

  it("omits absent run-command args and validates the plan", async () => {
    const seen: unknown[] = [];
    const api = visualApi({
      run: (command, args) => {
        seen.push([command, args]);
        return Promise.resolve({
          changes: [{ from: 0, to: 0, insert: "**" }],
          selection: { from: 2, to: 2 },
        }) as never;
      },
      runBinary: () => Promise.reject(new Error("unused")),
    });
    const plan = await api.runCommand({ session: 1, command: "bold", from: 0, to: 0 });
    expect(plan.selection).toEqual({ from: 2, to: 2 });
    expect(seen).toEqual([
      ["visual.run-command", { session: 1, command: "bold", from: 0, to: 0 }],
    ]);
  });
});

describe("errors and plans", () => {
  it("parses the [code] prefix", () => {
    expect(parseVisualError({ message: "[visual.desync] length mismatch" })).toEqual({
      code: "visual.desync",
      message: "length mismatch",
    });
    expect(parseVisualError(new Error("boom"))).toEqual({
      code: null,
      message: "boom",
    });
  });

  it("refuses a malformed plan", () => {
    expect(() => normalizePlan(null)).toThrow();
    expect(() =>
      normalizePlan({ changes: [{ from: "0", to: 1, insert: "" }] }),
    ).toThrow();
    expect(normalizePlan({ changes: [], selection: null })).toEqual({
      changes: [],
      selection: null,
    });
  });

  it("checks that a plan fits the document", () => {
    const plan = {
      changes: [
        { from: 0, to: 0, insert: "**" },
        { from: 4, to: 4, insert: "**" },
      ],
      selection: { from: 2, to: 6 },
    };
    expect(planFits(plan, 4)).toBe(true);
    expect(planFits(plan, 3)).toBe(false);
    expect(
      planFits(
        {
          changes: [
            { from: 3, to: 3, insert: "" },
            { from: 1, to: 1, insert: "" },
          ],
          selection: null,
        },
        4,
      ),
    ).toBe(false);
    expect(planFits({ changes: [], selection: { from: 0, to: 9 } }, 4)).toBe(false);
  });
});
