# Manang relay (Cloudflare Worker)

The hosted relay for Manang live editing: a Cloudflare Worker that sends each
room to one SQLite-backed Durable Object (WebSocket Hibernation API), which
forwards every binary frame to the other sockets of the room. It speaks the same
wire contract as the self-hosted `crates/manang-relay` binary, so the app's
`RelayClient` works against either (see `docs/adr/0013-live-editing.md`).

## What it sees

Ciphertext and metadata only. Frames are XChaCha20-Poly1305 envelopes keyed by a
secret that lives in the invite's `#fragment` and never reaches the relay. The
Worker checks a frame's length and its first two bytes (version, kind) and
forwards the rest untouched. It cannot see text, names, carets, file names or the
key. It can see that a connection to a room id exists, frame sizes and timing,
and the client address (to rate-limit it; only a short hash is kept, in memory,
for at most ten minutes). It stores nothing: no document state, no backlog, no
room list. `observability` is off in `wrangler.toml` and the code never logs.

## Wire contract (same as the Rust relay)

- `GET /room/<32 lowercase hex>` with `Upgrade: websocket`. Any other path is
  404 and nothing else under `/room` works. Binary messages only.
- A frame needs at least 42 bytes, byte 0 = 1 (version) and byte 1 = 1 (data) or
  2 (presence). It goes to every other socket in the room, never back to the
  sender. There is no replay for late joiners (the sync protocol between peers
  provides state, as with the Rust relay).
- `GET /health` answers `ok`; `GET /` is a plain page saying this is a relay.
- Close codes: 1013 room full (the socket is accepted, then closed, as in Rust),
  1002 not an envelope, 1003 text frame, 1008 rate limit, 1009 frame too large,
  1001 idle. HTTP: 404 bad path, 426 not an upgrade, 405 wrong method, 429 too
  many connections (with `Retry-After`).

## Limits (defaults, set in `[vars]` of `wrangler.toml`)

| Limit                       | Default     | Notes                                                                                   |
| --------------------------- | ----------- | --------------------------------------------------------------------------------------- |
| Frame size                  | 16 MiB      | as the Rust relay; Workers allow up to 32 MiB                                           |
| Peers per room              | 16          | as the Rust relay                                                                       |
| Messages / bytes per second | 200 / 4 MiB | per connection, two-second burst, as the Rust relay                                     |
| Presence messages / second  | 20          | per connection, as the Rust relay                                                       |
| Idle room                   | 15 min      | no frame and no join for this long closes the room (1001); clients reconnect and resync |
| Upgrades per address        | 60 / min    | 429 beyond; `0` turns it off                                                            |
| Distinct rooms per address  | 32 / 10 min | 429 beyond; a known room can always reconnect                                           |

Not carried over from the Rust relay: the global caps on rooms (1,024) and
connections (4,096), because Durable Objects have no cheap global counter; the
per-address limits and Cloudflare's own scaling stand in for them. Slow receivers
are not dropped; Cloudflare buffers outbound messages for them.

## Run it locally

```sh
cd relay/worker
pnpm install
pnpm test          # unit tests inside workerd (Workers runtime)
pnpm typecheck
pnpm dev           # wrangler dev on http://127.0.0.1:8787, nothing is deployed
pnpm conformance   # runs the Rust RelayClient test against wrangler dev
```

Point the app's `live.relay-url` at `ws://127.0.0.1:8787` to try it.
`pnpm conformance` needs a Rust toolchain; it builds only `manang-relay`.

## Deploy

```sh
cd relay/worker
pnpm install --frozen-lockfile
pnpm exec wrangler login        # or set CLOUDFLARE_API_TOKEN in your shell
pnpm exec wrangler deploy
```

The name in `wrangler.toml` (`manang-relay`) is a placeholder; change it if you
want another `<name>.<subdomain>.workers.dev` address. The relay address for
the app is `wss://<name>.<subdomain>.workers.dev`. Check it:
`curl https://<name>.<subdomain>.workers.dev/health` prints `ok`.

The Manang team's instance (deployed 2026-10-18) is `wss://manang-relay.fundamentswork.workers.dev`; it passes `crates/manang-relay/tests/conformance.rs` over `wss://`.

## Costs (read from Cloudflare's documentation on 2026-10-18)

From <https://developers.cloudflare.com/durable-objects/platform/pricing/>:
Free plan includes 100,000 requests/day and 13,000 GB-s/day; Paid includes 1
million requests and 400,000 GB-s per month, then $0.15 per million requests and
$12.50 per million GB-s. Incoming WebSocket messages count 20:1 as requests
(100 messages = 5 requests); outgoing messages and protocol pings are free. Idle
objects that can hibernate accrue no duration. Each new connection costs one
Worker request, one Durable Object request, and one more for the limiter. The
Worker's own request quota is on the Workers pricing page:
<https://developers.cloudflare.com/workers/platform/pricing/>. Limits such as
sockets per object and message size are on
<https://developers.cloudflare.com/durable-objects/platform/limits/>. This relay
writes no data to Durable Object storage except the idle-check alarm.

## Rotate or delete

- Rotate: nothing secret is stored, so there is nothing to rotate. Sessions are
  protected by the invite key; to cut everyone off, redeploy under a new name
  (new address) or delete the Worker.
- Delete: `pnpm exec wrangler delete` removes the Worker and its Durable
  Objects. Open sessions drop and clients reconnect to nothing.
- Update: `git pull` and `pnpm exec wrangler deploy` again. Do not edit the
  `v1` migration; add a new `[[migrations]]` tag for class changes.
