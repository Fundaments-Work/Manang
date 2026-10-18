// The relay's wire contract, mirrored from crates/manang-relay (server.rs,
// config.rs). Pure functions only, so they are unit-testable without a runtime.
//
// The relay sees ciphertext. These helpers look at the length and the first two
// bytes of a frame (the envelope version and kind) and nothing else.

/** manang_live::envelope::VERSION */
export const ENVELOPE_VERSION = 1;
/** HEADER_LEN (2 + 24-byte nonce) + TAG_LEN (16): the shortest valid envelope. */
export const MIN_ENVELOPE_BYTES = 42;

export const KIND_DATA = 1;
export const KIND_PRESENCE = 2;

/** Close codes, identical to the Rust relay's. */
export const CLOSE = {
  /** Idle timeout (Rust: CloseCode::Away). */
  AWAY: 1001,
  /** Not an envelope (Rust: Protocol). */
  PROTOCOL: 1002,
  /** Text frame (Rust: Unsupported). */
  UNSUPPORTED: 1003,
  /** Rate limit exceeded (Rust: Policy). */
  POLICY: 1008,
  /** Frame larger than the cap (RFC 6455 "message too big"). */
  TOO_BIG: 1009,
  /** Room is full, or the relay asks the client to retry (Rust: Again). */
  AGAIN: 1013,
} as const;

/** `/room/<32 lowercase hex>` and nothing else; returns the room id. */
export function parseRoom(path: string): string | null {
  const prefix = "/room/";
  if (!path.startsWith(prefix)) return null;
  const id = path.slice(prefix.length);
  return /^[0-9a-f]{32}$/.test(id) ? id : null;
}

/** What the relay can check without the key: kind 1 (data) or 2 (presence). */
export function plausibleEnvelope(frame: ArrayBuffer): number | null {
  if (frame.byteLength < MIN_ENVELOPE_BYTES) return null;
  const head = new Uint8Array(frame, 0, 2);
  if (head[0] !== ENVELOPE_VERSION) return null;
  return head[1] === KIND_DATA || head[1] === KIND_PRESENCE ? head[1] : null;
}

/** Token bucket, capacity two seconds of rate, as in the Rust relay. */
export class Bucket {
  private tokens: number;
  private last: number;
  private readonly capacity: number;

  constructor(
    private readonly perSec: number,
    now: number,
  ) {
    this.capacity = perSec * 2;
    this.tokens = this.capacity;
    this.last = now;
  }

  take(amount: number, now: number): boolean {
    const dt = Math.max(0, now - this.last) / 1000;
    this.last = now;
    this.tokens = Math.min(this.capacity, this.tokens + dt * this.perSec);
    if (this.tokens >= amount) {
      this.tokens -= amount;
      return true;
    }
    return false;
  }
}

export interface Env {
  ROOM: DurableObjectNamespace<import("./room").Room>;
  LIMITER: DurableObjectNamespace<import("./limiter").Limiter>;
  MAX_FRAME_BYTES?: string;
  MAX_PEERS_PER_ROOM?: string;
  RATE_MSGS_PER_SEC?: string;
  RATE_BYTES_PER_SEC?: string;
  RATE_PRESENCE_PER_SEC?: string;
  IDLE_TIMEOUT_SECS?: string;
  RATE_CONNECTS_PER_MIN?: string;
  RATE_ROOMS_PER_10MIN?: string;
}

export interface Limits {
  maxFrameBytes: number;
  maxPeersPerRoom: number;
  msgsPerSec: number;
  bytesPerSec: number;
  presencePerSec: number;
  idleMs: number;
  connectsPerMin: number;
  roomsPer10Min: number;
}

function num(value: string | undefined, fallback: number): number {
  if (value === undefined) return fallback;
  const n = Number(value);
  return Number.isFinite(n) && n >= 0 ? n : fallback;
}

/** Defaults equal the Rust relay's `RelayConfig::default()` where one exists. */
export function readLimits(env: Env): Limits {
  return {
    maxFrameBytes: num(env.MAX_FRAME_BYTES, 16 * 1024 * 1024),
    maxPeersPerRoom: num(env.MAX_PEERS_PER_ROOM, 16),
    msgsPerSec: num(env.RATE_MSGS_PER_SEC, 200),
    bytesPerSec: num(env.RATE_BYTES_PER_SEC, 4 * 1024 * 1024),
    presencePerSec: num(env.RATE_PRESENCE_PER_SEC, 20),
    idleMs: num(env.IDLE_TIMEOUT_SECS, 900) * 1000,
    connectsPerMin: num(env.RATE_CONNECTS_PER_MIN, 60),
    roomsPer10Min: num(env.RATE_ROOMS_PER_10MIN, 32),
  };
}
