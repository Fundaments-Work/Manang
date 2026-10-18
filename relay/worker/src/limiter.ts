// A lightweight abuse limiter, in memory only. It is keyed by a short hash of
// the client address (never the address itself) and a short hash of the room id,
// kept for at most ten minutes and never written to storage. If it is evicted
// the counters simply start over, which is fine for a rate limit.

import { DurableObject } from "cloudflare:workers";
import type { Env } from "./protocol";

const MIN_MS = 60_000;
const ROOMS_WINDOW_MS = 600_000;
const MAX_ENTRIES = 20_000;

interface Entry {
  connectsStart: number;
  connects: number;
  roomsStart: number;
  rooms: Set<string>;
}

export interface Admission {
  ok: boolean;
  /** Seconds until the refused client may try again. */
  retryAfter: number;
}

export class Limiter extends DurableObject<Env> {
  private readonly entries = new Map<string, Entry>();
  private lastSweep = 0;

  /**
   * Admit one WebSocket upgrade. `connectsPerMin` and `roomsPer10Min` of 0
   * switch the corresponding check off.
   */
  admit(
    client: string,
    room: string,
    connectsPerMin: number,
    roomsPer10Min: number,
  ): Admission {
    const now = Date.now();
    this.sweep(now);
    let e = this.entries.get(client);
    if (!e) {
      e = { connectsStart: now, connects: 0, roomsStart: now, rooms: new Set() };
      this.entries.set(client, e);
    }
    if (now - e.connectsStart >= MIN_MS) {
      e.connectsStart = now;
      e.connects = 0;
    }
    if (now - e.roomsStart >= ROOMS_WINDOW_MS) {
      e.roomsStart = now;
      e.rooms.clear();
    }
    if (connectsPerMin > 0 && e.connects >= connectsPerMin) {
      return {
        ok: false,
        retryAfter: Math.ceil((e.connectsStart + MIN_MS - now) / 1000),
      };
    }
    if (roomsPer10Min > 0 && !e.rooms.has(room) && e.rooms.size >= roomsPer10Min) {
      return {
        ok: false,
        retryAfter: Math.ceil((e.roomsStart + ROOMS_WINDOW_MS - now) / 1000),
      };
    }
    e.connects += 1;
    e.rooms.add(room);
    return { ok: true, retryAfter: 0 };
  }

  private sweep(now: number): void {
    if (now - this.lastSweep < MIN_MS && this.entries.size < MAX_ENTRIES) return;
    this.lastSweep = now;
    for (const [k, e] of this.entries) {
      if (now - e.roomsStart >= ROOMS_WINDOW_MS && now - e.connectsStart >= MIN_MS) {
        this.entries.delete(k);
      }
    }
    // Still over the cap under a flood of addresses: start over.
    if (this.entries.size >= MAX_ENTRIES) this.entries.clear();
  }
}
