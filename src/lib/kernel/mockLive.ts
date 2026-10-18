/**
 * In-memory `live` plugin for the browser (`pnpm dev`, verify-ui, vitest).
 *
 * THIS IS A MOCK. There is no relay, no encryption and no other person: a
 * scripted collaborator is added and driven by the `dev.live-*` commands, so the
 * whole session flow can be exercised (sharing, joining, remote edits and
 * carets, stale pushes, a dropped link, an error). What it does reproduce
 * exactly is the contract of `crates/manang-live-plugin`: the commands and
 * their errors (`{code, message}` JSON), the version counter, which refuses a
 * push made against an old version (`accepted: false`), and the ordered
 * `live.remote` / `live.presence` / `live.state` events. Keep it in step with
 * that crate's `manifest.toml` and `driver.rs`.
 *
 * `LiveAuthority` is the part of the Rust session thread that the web view
 * depends on; the binding's tests run against it with random interleavings.
 */

import type { Args, KernelError } from "./client";
import type { MockPlugin, MockPluginFactory } from "./mock";
import type {
  LiveChange,
  LiveConnection,
  LivePeer,
  LivePushResult,
  LiveRemoteSelection,
} from "./liveApi";
import { scalarLength, utf16ToScalar } from "./offsets";

const COMMANDS: readonly (readonly [string, string])[] = [
  ["live.start", "Share this file live"],
  ["live.join", "Join a live session"],
  ["live.leave", "Leave the live session"],
  ["live.invite", "Copy the live invite"],
  ["live.status", "Live session status"],
  ["live.inspect", "Read what a live invite points at"],
  ["live.push", "Send local edits to the live session"],
  ["live.selection", "Share the caret with the live session"],
  ["live.snapshot", "Re-read the live document"],
  ["live.contributors", "List who edited in the live session"],
  ["live.configure", "Apply live editing settings"],
];

function liveFail(code: string, message: string): KernelError {
  return { message: JSON.stringify({ code, message }) };
}

/** Position `pos` after sequential replacements (a stable caret, like a CRDT cursor). */
export function mapThrough(pos: number, changes: readonly LiveChange[]): number {
  let p = pos;
  for (const c of changes) {
    const end = c.start + c.delete;
    if (p <= c.start) continue;
    p = p >= end ? p - c.delete + c.insert.length : c.start + c.insert.length;
  }
  return p;
}

function applyChange(text: string, c: LiveChange): string {
  if (c.start < 0 || c.delete < 0 || c.start + c.delete > text.length) {
    throw liveFail(
      "invalid",
      `change ${c.start}..${c.start + c.delete} is outside the document (${text.length} units)`,
    );
  }
  return text.slice(0, c.start) + c.insert + text.slice(c.start + c.delete);
}

export type Emitter = (topic: string, payload: unknown) => void;

interface MockPeer extends LivePeer {
  selection: { anchor: number; head: number } | null;
}

/** The session thread's document and version, as the web view sees it. */
export class LiveAuthority {
  text: string;
  version = 0;
  readonly peers: MockPeer[] = [];
  /** Set to make `push` also run `onApply` (the editor mirror). */
  onApply: ((before: string, change: LiveChange) => void) | undefined;

  constructor(
    readonly session: string,
    text: string,
    private readonly emit: Emitter,
  ) {
    this.text = text;
  }

  /** The view pushes edits made against `version`. */
  push(version: number, changes: readonly LiveChange[]): LivePushResult {
    if (version !== this.version) return { accepted: false, version: this.version };
    let failure: unknown;
    for (const change of changes) {
      try {
        const before = this.text;
        this.text = applyChange(before, change);
        this.onApply?.(before, change);
        for (const peer of this.peers) {
          if (peer.selection === null) continue;
          peer.selection = {
            anchor: mapThrough(peer.selection.anchor, [change]),
            head: mapThrough(peer.selection.head, [change]),
          };
        }
      } catch (error) {
        failure = error;
        break;
      }
    }
    this.version += 1;
    if (failure !== undefined) throw failure;
    this.emit("live.remote", {
      session: this.session,
      version: this.version,
      ack: true,
      changes: [],
    });
    this.#presence();
    return { accepted: true, version: this.version };
  }

  /** A collaborator's edit arrives (sequential changes, like a CRDT import). */
  remote(changes: readonly LiveChange[]): void {
    if (changes.length === 0) return;
    for (const change of changes) {
      const before = this.text;
      this.text = applyChange(before, change);
      this.onApply?.(before, change);
      for (const peer of this.peers) {
        if (peer.selection === null) continue;
        peer.selection = {
          anchor: mapThrough(peer.selection.anchor, [change]),
          head: mapThrough(peer.selection.head, [change]),
        };
      }
    }
    this.version += 1;
    this.emit("live.remote", {
      session: this.session,
      version: this.version,
      ack: false,
      changes,
    });
    this.#presence();
  }

  /** Move a collaborator's caret. */
  caret(peerId: string, selection: { anchor: number; head: number } | null): void {
    const peer = this.peers.find((p) => p.peer === peerId);
    if (peer === undefined) return;
    peer.selection = selection;
    this.#presence();
  }

  #presence(): void {
    const selections: LiveRemoteSelection[] = [];
    for (const peer of this.peers) {
      if (peer.selection === null) continue;
      selections.push({
        peer: peer.peer,
        name: peer.name,
        slot: peer.slot,
        anchor: Math.min(peer.selection.anchor, this.text.length),
        head: Math.min(peer.selection.head, this.text.length),
      });
    }
    this.emit("live.presence", {
      session: this.session,
      version: this.version,
      selections,
    });
  }
}

/* -------------------------------------------------------------------------- */
/* The plugin                                                                 */
/* -------------------------------------------------------------------------- */

interface Run {
  authority: LiveAuthority;
  doc: number;
  role: "host" | "guest";
  invite: string;
  file: string | null;
  relay: string;
  state: LiveConnection;
  attempt: number;
  error: string | null;
}

function fingerprint(text: string): string {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i += 1) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h.toString(16).padStart(8, "0");
}

function parseInvite(text: string): {
  relay: string;
  file: string | null;
  fragment: string;
} {
  let url: URL;
  try {
    url = new URL(text.trim());
  } catch {
    throw liveFail("bad-invite", "This invite is not valid: it is not a link");
  }
  if (url.protocol !== "manang-live:") {
    throw liveFail(
      "bad-invite",
      "This invite is not valid: it does not start with manang-live://",
    );
  }
  const relay = url.searchParams.get("relay");
  if (relay === null || relay === "") {
    throw liveFail("bad-invite", "This invite is not valid: it names no relay");
  }
  const fragment = url.hash.replace(/^#/, "");
  if (fragment === "") {
    throw liveFail("bad-invite", "This invite is not valid: the key is missing");
  }
  const file = url.searchParams.get("file");
  return { relay, file: file === "" ? null : file, fragment };
}

function normalizeRelay(input: string): string {
  const trimmed = input.trim();
  if (trimmed === "")
    throw liveFail("no-relay", "Set a relay in Settings to share live.");
  const withScheme = trimmed.includes("://") ? trimmed : `wss://${trimmed}`;
  let url: URL;
  try {
    url = new URL(withScheme);
  } catch {
    throw liveFail("bad-relay", "The relay address is not valid.");
  }
  const scheme =
    url.protocol === "ws:" || url.protocol === "http:"
      ? "ws"
      : url.protocol === "wss:" || url.protocol === "https:"
        ? "wss"
        : null;
  if (scheme === null) {
    throw liveFail("bad-relay", "The relay address is not valid: use ws:// or wss://.");
  }
  return `${scheme}://${url.host}`;
}

function numberArg(args: Args, key: string): number {
  const v = args[key];
  if (typeof v !== "number" || !Number.isInteger(v) || v < 0) {
    throw liveFail("invalid", `\`${key}\` must be an unsigned integer`);
  }
  return v;
}

/**
 * Invites made for a pretend host: fragment -> the host's start text. Shared by
 * every mock kernel in the page, like a relay both people can reach.
 */
const pretend = new Map<string, string>();

/** Tests and verify-ui construct this to drive the scripted collaborator. */
export const livePlugin: MockPluginFactory = (emit, host): MockPlugin => {
  {
    let relayUrl = "";
    let run: Run | null = null;
    let nextSession = 1;
    let nextPeer = 7001;
    let lagMs = 0;

    const send: Emitter = (topic, payload) => {
      if (lagMs <= 0) emit(topic, payload);
      else setTimeout(() => emit(topic, payload), lagMs);
    };

    const stateEvent = (r: Run): void =>
      send("live.state", {
        session: r.authority.session,
        state: r.state,
        attempt: r.attempt,
        error: r.error,
        peers: r.authority.peers.map(({ peer, name, slot }) => ({ peer, name, slot })),
      });

    const setState = (r: Run, state: LiveConnection, attempt = 0): void => {
      r.state = state;
      r.attempt = attempt;
      stateEvent(r);
    };

    const mirror = (doc: number) => (before: string, c: LiveChange) => {
      // The kernel editor indexes Unicode scalars.
      const from = utf16ToScalar(before, c.start);
      const to = utf16ToScalar(before, c.start + c.delete);
      if (to > from) host.call("editor.delete", { id: doc, from, to });
      if (c.insert.length > 0)
        host.call("editor.insert", { id: doc, at: from, text: c.insert });
    };

    const current = (): Run => {
      if (run === null) throw liveFail("not-live", "There is no live session.");
      return run;
    };

    const begin = (
      doc: number,
      role: "host" | "guest",
      text: string,
      invite: string,
      file: string | null,
      relay: string,
    ): Run => {
      const session = `mock${String(nextSession).padStart(4, "0")}`;
      nextSession += 1;
      const authority = new LiveAuthority(session, text, send);
      authority.onApply = mirror(doc);
      const r: Run = {
        authority,
        doc,
        role,
        invite,
        file,
        relay,
        state: "connecting",
        attempt: 0,
        error: null,
      };
      run = r;
      stateEvent(r);
      setTimeout(() => {
        if (run === r && r.state === "connecting") setState(r, "connected");
      }, 40);
      return r;
    };

    const text = (doc: number): string =>
      String(host.call("editor.text", { id: doc }) ?? "");

    const handlers: Record<string, (args: Args) => unknown> = {
      "live.configure": (args) => {
        const values = (args["values"] ?? {}) as Record<string, unknown>;
        if (typeof values["live.relay-url"] === "string")
          relayUrl = values["live.relay-url"];
        return { applied: Object.keys(values).length };
      },
      "live.status": () => {
        const relaySet = relayUrl.trim() !== "";
        if (run === null) return { active: false, relay_set: relaySet };
        const r = run;
        return {
          active: true,
          relay_set: relaySet,
          session: r.authority.session,
          role: r.role,
          doc: r.doc,
          file: r.file,
          relay: r.relay,
          state: r.state,
          attempt: r.attempt,
          error: r.error,
          version: r.authority.version,
          peers: r.authority.peers.map(({ peer, name, slot }) => ({
            peer,
            name,
            slot,
          })),
        };
      },
      "live.start": (args) => {
        if (run !== null) {
          throw liveFail(
            "already-live",
            "A live session is already running. Leave it first.",
          );
        }
        const relay = normalizeRelay(relayUrl);
        const doc = numberArg(args, "doc");
        const file = typeof args["file"] === "string" ? args["file"] : null;
        const start = text(doc);
        const fragment = `mock${fingerprint(start)}${nextSession.toString(16).padStart(8, "0")}`;
        pretend.set(fragment, start);
        const url = new URL("manang-live://join");
        url.searchParams.set("relay", relay);
        if (file !== null) url.searchParams.set("file", file);
        url.hash = fragment;
        const r = begin(doc, "host", start, url.toString(), file, relay);
        return {
          session: r.authority.session,
          version: 0,
          role: "host",
          invite: r.invite,
        };
      },
      "live.join": (args) => {
        if (run !== null) {
          throw liveFail(
            "already-live",
            "A live session is already running. Leave it first.",
          );
        }
        const doc = numberArg(args, "doc");
        const link = parseInvite(
          typeof args["invite"] === "string" ? args["invite"] : "",
        );
        const hostText = pretend.get(link.fragment);
        if (hostText === undefined) {
          throw liveFail(
            "bad-invite",
            "This invite is not valid: nobody is hosting it.",
          );
        }
        const local = text(doc);
        const same = `mock${fingerprint(local)}` === link.fragment.slice(0, 12);
        let reset: LiveChange | null = null;
        let initial = local;
        let stream = false;
        if (!same) {
          if (local !== "" && args["resolution"] !== "take-host") {
            throw liveFail(
              "differs",
              "Your copy of this file differs from the host's. Joining replaces it with the host's version; your copy is saved first.",
            );
          }
          if (local !== "") {
            const chars = scalarLength(local);
            host.call("editor.delete", { id: doc, from: 0, to: chars });
            reset = { start: 0, delete: local.length, insert: "" };
          }
          initial = "";
          stream = true;
        }
        const r = begin(
          doc,
          "guest",
          initial,
          args["invite"] as string,
          link.file,
          link.relay,
        );
        // The other person is in the room; the host's text arrives as an edit.
        r.authority.peers.push({
          peer: String(nextPeer),
          name: "Ada Lovelace",
          slot: 3,
          selection: null,
        });
        nextPeer += 1;
        if (stream) {
          setTimeout(() => {
            if (run === r)
              r.authority.remote([{ start: 0, delete: 0, insert: hostText }]);
          }, 120);
        }
        return {
          session: r.authority.session,
          version: 0,
          role: "guest",
          file: link.file,
          reset,
        };
      },
      "live.leave": () => {
        if (run === null) return { left: false };
        const r = run;
        run = null;
        r.state = "left";
        stateEvent(r);
        return { left: true };
      },
      "live.invite": () => {
        const r = current();
        return { invite: r.invite, file: r.file };
      },
      "live.inspect": (args) => {
        const link = parseInvite(
          typeof args["invite"] === "string" ? args["invite"] : "",
        );
        return { relay: link.relay, file: link.file };
      },
      "live.push": (args) => {
        const r = current();
        if (
          typeof args["session"] === "string" &&
          args["session"] !== r.authority.session
        ) {
          throw liveFail("not-live", "There is no live session.");
        }
        const changes = Array.isArray(args["changes"])
          ? (args["changes"] as LiveChange[])
          : [];
        return r.authority.push(numberArg(args, "version"), changes);
      },
      "live.selection": () => ({}),
      "live.snapshot": () => {
        const r = current();
        return { version: r.authority.version, text: r.authority.text };
      },
      "live.contributors": () => {
        const r = current();
        return {
          contributors: r.authority.peers.map((p) => ({ name: p.name, email: null })),
        };
      },

      /* -- Dev hooks: the scripted collaborator and the network ------------ */
      "dev.live-peer-join": (args) => {
        const r = current();
        const peer: MockPeer = {
          peer: String(nextPeer),
          name: typeof args["name"] === "string" ? args["name"] : "Ada Lovelace",
          slot: typeof args["slot"] === "number" ? args["slot"] : 3,
          selection: null,
        };
        nextPeer += 1;
        r.authority.peers.push(peer);
        stateEvent(r);
        return { peer: peer.peer };
      },
      "dev.live-peer-leave": (args) => {
        const r = current();
        const at = r.authority.peers.findIndex((p) => p.peer === String(args["peer"]));
        if (at >= 0) r.authority.peers.splice(at, 1);
        stateEvent(r);
        r.authority.caret("", null);
        return null;
      },
      "dev.live-peer-type": (args) => {
        const r = current();
        const changes = Array.isArray(args["changes"])
          ? (args["changes"] as LiveChange[])
          : [
              {
                start: numberArg(args, "at"),
                delete: typeof args["delete"] === "number" ? args["delete"] : 0,
                insert: typeof args["text"] === "string" ? args["text"] : "",
              },
            ];
        r.authority.remote(changes);
        return { version: r.authority.version };
      },
      "dev.live-peer-caret": (args) => {
        const r = current();
        r.authority.caret(String(args["peer"]), {
          anchor: numberArg(args, "anchor"),
          head: numberArg(args, "head"),
        });
        return null;
      },
      "dev.live-link": (args) => {
        const r = current();
        if (args["state"] === "down") setState(r, "reconnecting", 1);
        else setState(r, "connected");
        return null;
      },
      "dev.live-fail": (args) => {
        const r = current();
        r.error =
          typeof args["message"] === "string" ? args["message"] : "The session failed.";
        setState(r, "error");
        return null;
      },
      /** Delay every event by `ms`, so pushes made meanwhile are refused as stale. */
      "dev.live-lag": (args) => {
        lagMs = typeof args["ms"] === "number" ? args["ms"] : 0;
        return null;
      },
      /** An invite for a pretend host whose file holds `text`. */
      "dev.live-make-invite": (args) => {
        const start = typeof args["text"] === "string" ? args["text"] : "";
        const fragment = `mock${fingerprint(start)}${(1000 + pretend.size).toString(16).padStart(8, "0")}`;
        pretend.set(fragment, start);
        const url = new URL("manang-live://join");
        url.searchParams.set("relay", "wss://relay.example.org");
        if (typeof args["file"] === "string")
          url.searchParams.set("file", args["file"]);
        url.hash = fragment;
        return { invite: url.toString() };
      },
      "dev.live-text": () => (run === null ? null : run.authority.text),
    };

    return {
      name: "live",
      commands: COMMANDS,
      handlers,
    };
  }
};
