// Manang hosted relay: a Worker in front of one Durable Object per room.
//
// Same wire contract as crates/manang-relay:
//   GET /room/<32 lowercase hex>  upgrades to a WebSocket (binary frames only)
//   GET /health                   answers "ok"
// Anything else on /room is a 404. The relay only ever forwards ciphertext and
// writes no logs.

import { Limiter } from "./limiter";
import { parseRoom, readLimits, type Env } from "./protocol";
import { Room } from "./room";

export { Limiter, Room };

const PAGE = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex">
<title>Manang relay</title>
<style>body{font:16px/1.5 system-ui,sans-serif;max-width:36rem;margin:4rem auto;padding:0 1rem}</style>
</head>
<body>
<h1>Manang relay</h1>
<p>This is a relay for Manang live editing. It passes end-to-end encrypted messages between the people in a shared session and cannot read them. It stores nothing, runs no analytics and sets no cookies.</p>
<p>There is nothing to see here. Set this address as the relay in Manang's Live settings.</p>
</body>
</html>
`;

const PLAIN = {
  "content-type": "text/plain; charset=utf-8",
  "cache-control": "no-store",
};

/** First 8 bytes of SHA-256, hex: enough to bucket by, useless to recover from. */
async function shortHash(text: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return [...new Uint8Array(digest, 0, 8)]
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    const readOnly = request.method === "GET" || request.method === "HEAD";

    if (url.pathname === "/health") {
      return readOnly
        ? new Response(request.method === "HEAD" ? null : "ok\n", { headers: PLAIN })
        : new Response("method not allowed", { status: 405, headers: PLAIN });
    }
    if (url.pathname === "/") {
      return readOnly
        ? new Response(request.method === "HEAD" ? null : PAGE, {
            headers: {
              "content-type": "text/html; charset=utf-8",
              "cache-control": "no-store",
            },
          })
        : new Response("method not allowed", { status: 405, headers: PLAIN });
    }

    const room = parseRoom(url.pathname);
    if (room === null) {
      return new Response("expected /room/<32 hex>", { status: 404, headers: PLAIN });
    }
    if (request.method !== "GET") {
      return new Response("method not allowed", { status: 405, headers: PLAIN });
    }
    if (request.headers.get("Upgrade")?.toLowerCase() !== "websocket") {
      return new Response("expected a WebSocket upgrade", {
        status: 426,
        headers: PLAIN,
      });
    }

    const limits = readLimits(env);
    if (limits.connectsPerMin > 0 || limits.roomsPer10Min > 0) {
      const client = (
        await shortHash(request.headers.get("CF-Connecting-IP") ?? "local")
      ).slice(0, 16);
      try {
        const shard = Number.parseInt(client.slice(0, 2), 16) % 4;
        const limiter = env.LIMITER.get(env.LIMITER.idFromName(`shard-${shard}`));
        const verdict = await limiter.admit(
          client,
          await shortHash(room),
          limits.connectsPerMin,
          limits.roomsPer10Min,
        );
        if (!verdict.ok) {
          return new Response("too many connections; slow down", {
            status: 429,
            headers: { ...PLAIN, "retry-after": String(verdict.retryAfter) },
          });
        }
      } catch {
        // The limiter is best effort: if it is unavailable, let the client in.
      }
    }

    return env.ROOM.get(env.ROOM.idFromName(room)).fetch(request);
  },
} satisfies ExportedHandler<Env>;
