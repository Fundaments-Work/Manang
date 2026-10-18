// One Durable Object per room. It holds no document state: only the live
// sockets (WebSocket Hibernation API) and, in memory, per-socket rate buckets.
// Every binary frame is forwarded to the other sockets of the room, never
// echoed back, exactly as crates/manang-relay does. There is no backlog: a late
// joiner gets its state from the peers through the sync protocol, as with the
// Rust relay.
//
// Nothing here logs or stores a frame, a room id or an address.

import { DurableObject } from "cloudflare:workers";
import {
  Bucket,
  CLOSE,
  KIND_PRESENCE,
  plausibleEnvelope,
  readLimits,
  type Env,
  type Limits,
} from "./protocol";

/** Survives hibernation (serializeAttachment); the only per-socket state kept. */
interface Attachment {
  seen: number;
}

interface Live {
  seen: number;
  persisted: number;
  bytes: Bucket;
  msgs: Bucket;
  presence: Bucket;
}

/** How often the "last seen" mark is written to the socket attachment. */
const PERSIST_EVERY_MS = 5_000;

export class Room extends DurableObject<Env> {
  private readonly limits: Limits;
  private readonly live = new WeakMap<WebSocket, Live>();
  private alarmArmed = false;

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    this.limits = readLimits(env);
  }

  override async fetch(request: Request): Promise<Response> {
    if (request.headers.get("Upgrade")?.toLowerCase() !== "websocket") {
      return new Response("expected a WebSocket upgrade", { status: 426 });
    }
    const [client, server] = Object.values(new WebSocketPair()) as [
      WebSocket,
      WebSocket,
    ];
    const full = this.ctx.getWebSockets().length >= this.limits.maxPeersPerRoom;
    this.ctx.acceptWebSocket(server);
    if (full) {
      // Like the Rust relay: complete the handshake, then say why we hang up.
      server.close(CLOSE.AGAIN, "room is full");
    } else {
      server.serializeAttachment({ seen: Date.now() } satisfies Attachment);
      await this.armAlarm();
    }
    return new Response(null, { status: 101, webSocket: client });
  }

  override async webSocketMessage(
    ws: WebSocket,
    message: string | ArrayBuffer,
  ): Promise<void> {
    if (typeof message === "string") {
      ws.close(CLOSE.UNSUPPORTED, "binary frames only");
      return;
    }
    if (message.byteLength > this.limits.maxFrameBytes) {
      ws.close(CLOSE.TOO_BIG, "frame too large");
      return;
    }
    const kind = plausibleEnvelope(message);
    if (kind === null) {
      ws.close(CLOSE.PROTOCOL, "not an envelope");
      return;
    }
    const now = Date.now();
    const state = this.touch(ws, now);
    const within =
      state.bytes.take(message.byteLength, now) &&
      state.msgs.take(1, now) &&
      (kind !== KIND_PRESENCE || state.presence.take(1, now));
    if (!within) {
      ws.close(CLOSE.POLICY, "rate limit exceeded");
      return;
    }
    for (const peer of this.ctx.getWebSockets()) {
      if (peer === ws || peer.readyState !== WebSocket.READY_STATE_OPEN) continue;
      try {
        peer.send(message);
      } catch {
        // A peer that cannot be written to is closing; its own close event
        // removes it. The sender is unaffected.
      }
    }
  }

  override async webSocketClose(ws: WebSocket, code: number): Promise<void> {
    // Echo a valid code back; 1005/1006/1015 are not allowed on the wire.
    try {
      ws.close(
        code >= 1000 && code < 5000 && code !== 1005 && code !== 1006 ? code : 1000,
      );
    } catch {
      // already closed
    }
    await this.afterLeave(ws);
  }

  override async webSocketError(ws: WebSocket): Promise<void> {
    try {
      ws.close(1011, "error");
    } catch {
      // already closed
    }
    await this.afterLeave(ws);
  }

  /**
   * Close the whole room if no frame has flowed (and nobody has joined) for the
   * idle timeout; otherwise re-arm for when it would next become idle. Quiet
   * listeners are not idle while someone in the room is still sending.
   */
  override async alarm(): Promise<void> {
    this.alarmArmed = false;
    const now = Date.now();
    const open = this.ctx
      .getWebSockets()
      .filter((s) => s.readyState === WebSocket.READY_STATE_OPEN);
    if (open.length === 0) return;
    const latest = Math.max(...open.map((ws) => this.seenOf(ws)));
    if (now - latest >= this.limits.idleMs) {
      for (const ws of open) {
        try {
          ws.close(CLOSE.AWAY, "idle");
        } catch {
          // already closed
        }
      }
      return;
    }
    await this.ctx.storage.setAlarm(latest + this.limits.idleMs + 1_000);
    this.alarmArmed = true;
  }

  private seenOf(ws: WebSocket): number {
    const live = this.live.get(ws);
    if (live) return live.seen;
    const att = ws.deserializeAttachment() as Attachment | null;
    return att?.seen ?? Date.now();
  }

  /** Record activity and return the socket's buckets (fresh after a hibernation). */
  private touch(ws: WebSocket, now: number): Live {
    let live = this.live.get(ws);
    if (!live) {
      const l = this.limits;
      live = {
        seen: now,
        persisted: this.seenOf(ws),
        bytes: new Bucket(l.bytesPerSec, now),
        msgs: new Bucket(l.msgsPerSec, now),
        presence: new Bucket(l.presencePerSec, now),
      };
      this.live.set(ws, live);
    }
    live.seen = now;
    if (now - live.persisted >= PERSIST_EVERY_MS) {
      ws.serializeAttachment({ seen: now } satisfies Attachment);
      live.persisted = now;
    }
    return live;
  }

  private async armAlarm(): Promise<void> {
    if (this.alarmArmed) return;
    if ((await this.ctx.storage.getAlarm()) === null) {
      await this.ctx.storage.setAlarm(Date.now() + this.limits.idleMs + 1_000);
    }
    this.alarmArmed = true;
  }

  /** The room is forgotten when the last socket leaves: nothing is kept. */
  private async afterLeave(leaving: WebSocket): Promise<void> {
    const remaining = this.ctx.getWebSockets().filter((s) => s !== leaving);
    if (remaining.length === 0) {
      await this.ctx.storage.deleteAlarm();
      this.alarmArmed = false;
    }
  }
}
