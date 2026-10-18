import { describe, expect, it } from "vitest";
import { Bucket, parseRoom, plausibleEnvelope } from "../src/protocol";

function frame(len: number, version = 1, kind = 1): ArrayBuffer {
  const b = new Uint8Array(len);
  b[0] = version;
  b[1] = kind;
  return b.buffer;
}

describe("parseRoom", () => {
  it("accepts exactly 32 lowercase hex under /room/", () => {
    const id = "0123456789abcdef0123456789abcdef";
    expect(parseRoom(`/room/${id}`)).toBe(id);
  });

  it.each([
    "/room/",
    "/room/0123",
    "/room/0123456789ABCDEF0123456789ABCDEF",
    "/room/0123456789abcdef0123456789abcdeg",
    "/room/0123456789abcdef0123456789abcdef/",
    "/room/0123456789abcdef0123456789abcdef0",
    "/rooms/0123456789abcdef0123456789abcdef",
    "/",
  ])("rejects %s", (path) => {
    expect(parseRoom(path)).toBeNull();
  });
});

describe("plausibleEnvelope", () => {
  it("accepts data and presence of at least 42 bytes", () => {
    expect(plausibleEnvelope(frame(42, 1, 1))).toBe(1);
    expect(plausibleEnvelope(frame(100, 1, 2))).toBe(2);
  });

  it("rejects short frames, wrong versions and unknown kinds", () => {
    expect(plausibleEnvelope(frame(41))).toBeNull();
    expect(plausibleEnvelope(frame(60, 2, 1))).toBeNull();
    expect(plausibleEnvelope(frame(60, 1, 3))).toBeNull();
    expect(plausibleEnvelope(frame(60, 1, 0))).toBeNull();
  });
});

describe("Bucket", () => {
  it("starts with two seconds of burst and refills at the rate", () => {
    const b = new Bucket(10, 0);
    for (let i = 0; i < 20; i++) expect(b.take(1, 0)).toBe(true);
    expect(b.take(1, 0)).toBe(false);
    expect(b.take(1, 100)).toBe(true); // 0.1 s refills one token
    expect(b.take(1, 100)).toBe(false);
  });
});
