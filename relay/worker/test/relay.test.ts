import { env as rawEnv } from "cloudflare:test";
import { exports } from "cloudflare:workers";
import { afterEach, describe, expect, it } from "vitest";
import worker from "../src/index";
import type { Env } from "../src/protocol";

const env = rawEnv as unknown as Env;
const app = (exports as unknown as { default: Fetcher }).default;

// Limits lowered in vitest.config.ts: frame 4096, 3 peers, 20 msg/s,
// 2 presence/s, idle 2 s.

const ROOM = "0123456789abcdef0123456789abcdef";
const OTHER_ROOM = "fedcba9876543210fedcba9876543210";

/** A valid-looking envelope: version 1, the kind, then `len - 2` filler bytes. */
function envelope(len: number, kind = 1, fill = 7): ArrayBuffer {
  const b = new Uint8Array(len).fill(fill);
  b[0] = 1;
  b[1] = kind;
  return b.buffer;
}

interface Peer {
  ws: WebSocket;
  frames: ArrayBuffer[];
  closed: Promise<{ code: number; reason: string }>;
  next(): Promise<ArrayBuffer>;
}

const open: WebSocket[] = [];

async function connect(room = ROOM, path = `/room/${room}`): Promise<Peer> {
  const res = await app.fetch(`https://relay.test${path}`, {
    headers: { Upgrade: "websocket" },
  });
  expect(res.status).toBe(101);
  const ws = res.webSocket;
  if (!ws) throw new Error("no websocket");
  ws.binaryType = "arraybuffer";
  ws.accept();
  open.push(ws);
  const frames: ArrayBuffer[] = [];
  const waiters: ((f: ArrayBuffer) => void)[] = [];
  ws.addEventListener("message", (ev: MessageEvent) => {
    const data = ev.data as ArrayBuffer;
    const w = waiters.shift();
    if (w) w(data);
    else frames.push(data);
  });
  const closed = new Promise<{ code: number; reason: string }>((resolve) => {
    ws.addEventListener("close", (ev: CloseEvent) =>
      resolve({ code: ev.code, reason: ev.reason }),
    );
  });
  return {
    ws,
    frames,
    closed,
    next: () =>
      new Promise((resolve, reject) => {
        const queued = frames.shift();
        if (queued) return resolve(queued);
        const t = setTimeout(() => reject(new Error("no frame within 2 s")), 2000);
        waiters.push((f) => {
          clearTimeout(t);
          resolve(f);
        });
      }),
  };
}

const settle = (ms = 150) => new Promise((r) => setTimeout(r, ms));

afterEach(async () => {
  for (const ws of open.splice(0)) {
    try {
      ws.close(1000);
    } catch {
      // already closed
    }
  }
  await settle(50);
});

describe("HTTP surface", () => {
  it("answers /health with ok", async () => {
    const res = await app.fetch("https://relay.test/health");
    expect(res.status).toBe(200);
    expect(await res.text()).toBe("ok\n");
  });

  it("serves a plain page at / and nothing tracking-related", async () => {
    const res = await app.fetch("https://relay.test/");
    expect(res.status).toBe(200);
    const body = await res.text();
    expect(body).toContain("Manang relay");
    expect(body).not.toMatch(/<script|https?:\/\//i);
    expect(res.headers.get("set-cookie")).toBeNull();
  });

  it.each([
    "/room",
    "/room/",
    "/room/abc",
    "/room/0123456789ABCDEF0123456789ABCDEF",
    `/room/${ROOM}/extra`,
    "/rooms/" + ROOM,
    "/other",
  ])("refuses the path %s with 404", async (path) => {
    const res = await app.fetch(`https://relay.test${path}`, {
      headers: { Upgrade: "websocket" },
    });
    expect(res.status).toBe(404);
  });

  it("requires an upgrade on a room path", async () => {
    const res = await app.fetch(`https://relay.test/room/${ROOM}`);
    expect(res.status).toBe(426);
  });

  it("refuses non-GET methods on a room path", async () => {
    const res = await app.fetch(`https://relay.test/room/${ROOM}`, {
      method: "POST",
    });
    expect(res.status).toBe(405);
  });
});

describe("fan-out", () => {
  it("forwards a frame to the other peers, byte for byte, and never echoes it", async () => {
    const a = await connect();
    const b = await connect();
    const c = await connect();
    const sent = envelope(100, 1, 0xab);
    a.ws.send(sent);
    expect(new Uint8Array(await b.next())).toEqual(new Uint8Array(sent));
    expect(new Uint8Array(await c.next())).toEqual(new Uint8Array(sent));
    await settle();
    expect(a.frames).toHaveLength(0);
  });

  it("keeps rooms apart", async () => {
    const a = await connect(ROOM);
    const b = await connect(ROOM);
    const other = await connect(OTHER_ROOM);
    a.ws.send(envelope(80, 1));
    await b.next();
    await settle();
    expect(other.frames).toHaveLength(0);
  });

  it("forwards presence frames and keeps their order", async () => {
    const a = await connect();
    const b = await connect();
    a.ws.send(envelope(60, 1, 1));
    a.ws.send(envelope(60, 2, 2));
    a.ws.send(envelope(60, 1, 3));
    const seen = [new Uint8Array(await b.next()), new Uint8Array(await b.next())];
    seen.push(new Uint8Array(await b.next()));
    expect(seen.map((f) => [f[1], f[2]])).toEqual([
      [1, 1],
      [2, 2],
      [1, 3],
    ]);
  });

  it("has no backlog: a late joiner receives only later frames", async () => {
    const a = await connect();
    const b = await connect();
    a.ws.send(envelope(60, 1, 9));
    await b.next();
    const late = await connect();
    await settle();
    expect(late.frames).toHaveLength(0);
    a.ws.send(envelope(60, 1, 10));
    expect(new Uint8Array(await late.next())[2]).toBe(10);
  });

  it("lets a peer leave and the others carry on", async () => {
    const a = await connect();
    const b = await connect();
    const c = await connect();
    b.ws.close(1000, "bye");
    await b.closed;
    a.ws.send(envelope(60, 1, 5));
    expect(new Uint8Array(await c.next())[2]).toBe(5);
  });
});

describe("limits and close codes", () => {
  it("closes a text message with 1003", async () => {
    const a = await connect();
    a.ws.send("hello");
    expect((await a.closed).code).toBe(1003);
  });

  it.each([
    ["too short", () => envelope(41, 1)],
    ["wrong version", () => new Uint8Array(60).fill(0).buffer],
    ["unknown kind", () => envelope(60, 9)],
  ])("closes a frame that is not an envelope (%s) with 1002", async (_n, make) => {
    const a = await connect();
    a.ws.send(make());
    expect((await a.closed).code).toBe(1002);
  });

  it("closes an over-sized frame with 1009 and forwards nothing", async () => {
    const a = await connect();
    const b = await connect();
    a.ws.send(envelope(4097, 1));
    expect((await a.closed).code).toBe(1009);
    await settle();
    expect(b.frames).toHaveLength(0);
  });

  it("accepts a frame exactly at the cap", async () => {
    const a = await connect();
    const b = await connect();
    a.ws.send(envelope(4096, 1));
    expect((await b.next()).byteLength).toBe(4096);
  });

  it("closes a flooder with 1008 but not a polite peer", async () => {
    const polite = await connect();
    const flood = await connect();
    polite.ws.send(envelope(60, 1));
    for (let i = 0; i < 100; i++) {
      try {
        flood.ws.send(envelope(60, 1));
      } catch {
        break;
      }
    }
    expect((await flood.closed).code).toBe(1008);
    expect(polite.ws.readyState).toBe(WebSocket.READY_STATE_OPEN);
  });

  it("limits presence frames more tightly than data", async () => {
    const a = await connect();
    // Presence budget is 2/s with a 4-frame burst: the fifth is refused.
    for (let i = 0; i < 6; i++) {
      try {
        a.ws.send(envelope(60, 2));
      } catch {
        break;
      }
    }
    expect((await a.closed).code).toBe(1008);
  });

  it("refuses the peer beyond the room limit with 1013 and keeps the others", async () => {
    const a = await connect();
    const b = await connect();
    const c = await connect();
    const extra = await connect();
    expect(await extra.closed).toMatchObject({ code: 1013, reason: "room is full" });
    a.ws.send(envelope(60, 1, 4));
    expect(new Uint8Array(await b.next())[2]).toBe(4);
    expect(new Uint8Array(await c.next())[2]).toBe(4);
    expect(extra.frames).toHaveLength(0);
  });

  it("frees a slot when a peer leaves", async () => {
    const a = await connect();
    await connect();
    await connect();
    a.ws.close(1000);
    await a.closed;
    await settle();
    const d = await connect();
    d.ws.send(envelope(60, 1));
    await settle();
    expect(d.ws.readyState).toBe(WebSocket.READY_STATE_OPEN);
  });

  it("closes a room that has been silent past the idle timeout with 1001", async () => {
    const a = await connect();
    const b = await connect();
    // Idle timeout is 2 s; the alarm fires a second later.
    expect(await a.closed).toMatchObject({ code: 1001, reason: "idle" });
    expect((await b.closed).code).toBe(1001);
  }, 10_000);

  it("keeps a room alive while frames flow", async () => {
    const a = await connect();
    const b = await connect();
    for (let i = 0; i < 4; i++) {
      await settle(800);
      a.ws.send(envelope(60, 1));
      await b.next();
    }
    expect(a.ws.readyState).toBe(WebSocket.READY_STATE_OPEN);
  }, 10_000);
});

describe("per-address rate limit", () => {
  const limited = (connects: string, rooms: string) => ({
    ROOM: env.ROOM,
    LIMITER: env.LIMITER,
    RATE_CONNECTS_PER_MIN: connects,
    RATE_ROOMS_PER_10MIN: rooms,
  });
  const upgrade = (room: string, ip: string) =>
    new Request(`https://relay.test/room/${room}`, {
      headers: { Upgrade: "websocket", "CF-Connecting-IP": ip },
    });

  it("answers 429 with Retry-After after too many upgrades from one address", async () => {
    const e = limited("2", "0");
    const ip = "203.0.113.1";
    for (let i = 0; i < 2; i++) {
      const res = await worker.fetch(upgrade(ROOM, ip), e as never);
      expect(res.status).toBe(101);
      res.webSocket?.accept();
      res.webSocket?.close(1000);
    }
    const refused = await worker.fetch(upgrade(ROOM, ip), e as never);
    expect(refused.status).toBe(429);
    expect(Number(refused.headers.get("retry-after"))).toBeGreaterThan(0);
    // Another address is unaffected.
    const fine = await worker.fetch(upgrade(ROOM, "203.0.113.2"), e as never);
    expect(fine.status).toBe(101);
    fine.webSocket?.accept();
    fine.webSocket?.close(1000);
  });

  it("limits distinct rooms per address but lets a known room reconnect", async () => {
    const e = limited("0", "2");
    const ip = "203.0.113.3";
    const rooms = ["a".repeat(32), "b".repeat(32), "c".repeat(32)];
    for (const room of rooms.slice(0, 2)) {
      const res = await worker.fetch(upgrade(room, ip), e as never);
      expect(res.status).toBe(101);
      res.webSocket?.accept();
      res.webSocket?.close(1000);
    }
    expect((await worker.fetch(upgrade(rooms[2] ?? "", ip), e as never)).status).toBe(
      429,
    );
    const again = await worker.fetch(upgrade(rooms[0] ?? "", ip), e as never);
    expect(again.status).toBe(101);
    again.webSocket?.accept();
    again.webSocket?.close(1000);
  });
});
