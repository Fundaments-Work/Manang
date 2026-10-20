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
 * Project sessions (`scope: "project"`) are mocked too: a tree of text files,
 * each with its own `LiveAuthority` (own version counter, events carry `path`),
 * the two-phase join (`live.plan`, `live.resolve`), per-file attach/detach, tree
 * operations and notices from scripted peers. The host's files are seeded with
 * `dev.live-seed`, the guest's disk with `dev.live-local`. It also follows the
 * real project driver where that is stricter than the first mock was: a project
 * command needs the open folder's `root` (else `no-project`), replacing more
 * than one file needs the `*` confirmation (`needs-confirmation`), commands
 * other than resolve/leave are refused before the session is `live` (`waiting`
 * while the manifest is awaited, `invalid` while a plan waits), and a large
 * change held on disk (`dev.live-paused`) is answered with
 * `live.resync {paused}` (a plain resync meanwhile is refused as `paused`).
 *
 * `LiveAuthority` is the part of the Rust session thread that the web view
 * depends on; the binding's tests run against it with random interleavings.
 */

import type { Args, KernelError } from "./client";
import type { MockPlugin, MockPluginFactory } from "./mock";
import type {
  LiveChange,
  LiveChoice,
  LiveConnection,
  LiveFileNode,
  LivePausedInfo,
  LivePeer,
  LivePhase,
  LivePlanClass,
  LivePlanFile,
  LiveProgress,
  LivePushResult,
  LiveRemoteSelection,
  LiveScope,
  LiveTreeOp,
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
  ["live.resync", "Rebuild the live document from the others"],
  ["live.configure", "Apply live editing settings"],
  ["live.resolve", "Apply the choices for joining a live project"],
  ["live.attach", "Start editing a file of the live session"],
  ["live.detach", "Stop editing a file of the live session"],
  ["live.follow", "Follow a person in the live session"],
  ["live.files", "List the files of the live session"],
  ["live.exclude", "Keep a file out of the live session"],
  ["live.include", "Put a file back into the live session"],
  ["live.restore", "Restore a file deleted in the live session"],
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
  /** The file in front of the peer (project sessions). */
  path: string | null;
}

/** The session thread's document and version, as the web view sees it. */
export class LiveAuthority {
  text: string;
  version = 0;
  /** Set to make `push` also run `onApply` (the editor mirror). */
  onApply: ((before: string, change: LiveChange) => void) | undefined;

  constructor(
    readonly session: string,
    text: string,
    private readonly emit: Emitter,
    /** Shared by all files of a project session. */
    readonly peers: MockPeer[] = [],
    /** The file, in a project session: events name it. */
    readonly path?: string,
  ) {
    this.text = text;
  }

  #at(peer: MockPeer): boolean {
    return this.path === undefined || peer.path === this.path;
  }

  #where(): { path?: string } {
    return this.path === undefined ? {} : { path: this.path };
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
          if (peer.selection === null || !this.#at(peer)) continue;
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
      ...this.#where(),
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
        if (peer.selection === null || !this.#at(peer)) continue;
        peer.selection = {
          anchor: mapThrough(peer.selection.anchor, [change]),
          head: mapThrough(peer.selection.head, [change]),
        };
      }
    }
    this.version += 1;
    this.emit("live.remote", {
      session: this.session,
      ...this.#where(),
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
      if (peer.selection === null || !this.#at(peer)) continue;
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
      ...this.#where(),
      version: this.version,
      selections,
    });
  }
}

/* -------------------------------------------------------------------------- */
/* The plugin                                                                 */
/* -------------------------------------------------------------------------- */

/** The tree of a project session, as the plugin holds it. */
interface MockProject {
  /** Files in the session, each with its own document and version. */
  files: Map<string, LiveAuthority>;
  /** The host's files a guest has been offered (before `live.resolve`). */
  offered: Map<string, string>;
  /** The guest's own files, to classify against the offer. */
  local: Map<string, string>;
  plan: LivePlanFile[] | null;
  attached: Set<string>;
  excluded: Set<string>;
  /** Deleted by a peer: path -> the last text, until restored or the session ends. */
  deleted: Map<string, string>;
  progress: LiveProgress | null;
  following: string | null;
  /** A guest waits for the host's file list, for its own choices, or is in. */
  phase: LivePhase;
  /** Files no one present has shown they hold (what a host strands by leaving). */
  unsynced: number | null;
  /** A held large change on disk, until `live.resync {paused}` answers it. */
  paused: LivePausedInfo | null;
}

interface Run {
  scope: LiveScope;
  /** The document of a file session; for a project, an unused empty one. */
  authority: LiveAuthority;
  peers: MockPeer[];
  project: MockProject | null;
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
  scope: LiveScope;
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
  const scope: LiveScope =
    url.searchParams.get("scope") === "project" ? "project" : "file";
  return { relay, file: file === "" ? null : file, fragment, scope };
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
/** Invites made for a pretend project host: fragment -> the host's files. */
const pretendProjects = new Map<string, Map<string, string>>();

function classify(
  offered: ReadonlyMap<string, string>,
  local: ReadonlyMap<string, string>,
): LivePlanFile[] {
  const out: LivePlanFile[] = [];
  for (const [path, text] of offered) {
    const mine = local.get(path);
    const cls: LivePlanClass =
      mine === undefined ? "missing" : mine === text ? "identical" : "differing";
    out.push({ path, class: cls });
  }
  for (const path of local.keys()) {
    if (!offered.has(path)) out.push({ path, class: "guest-only" });
  }
  return out;
}

function unsafePath(path: string): boolean {
  return (
    path === "" ||
    path.startsWith("/") ||
    path.split("/").some((part) => part === ".." || part === "." || part === "") ||
    path.split("/").some((part) => part.startsWith("."))
  );
}

/** Tests and verify-ui construct this to drive the scripted collaborator. */
export const livePlugin: MockPluginFactory = (emit, host): MockPlugin => {
  {
    let relayUrl = "";
    let run: Run | null = null;
    let nextSession = 1;
    let nextPeer = 7001;
    let lagMs = 0;
    /** The host's files for the next `live.start` of a project. */
    let seeded = new Map<string, string>();
    /** The guest's disk for the next project join. */
    let localDisk = new Map<string, string>();

    const send: Emitter = (topic, payload) => {
      if (lagMs <= 0) emit(topic, payload);
      else setTimeout(() => emit(topic, payload), lagMs);
    };

    const peerList = (r: Run) =>
      r.peers.map(({ peer, name, slot, path }) =>
        r.scope === "project" ? { peer, name, slot, path } : { peer, name, slot },
      );

    const stateEvent = (r: Run): void =>
      send("live.state", {
        session: r.authority.session,
        state: r.state,
        attempt: r.attempt,
        error: r.error,
        peers: peerList(r),
      });

    /** Files no one present has shown they hold: all of them while alone. */
    const unsyncedOf = (r: Run): number => {
      const project = r.project;
      if (project === null) return 0;
      return project.unsynced ?? (r.peers.length === 0 ? project.files.size : 0);
    };

    const peersEvent = (r: Run): void => {
      if (r.scope !== "project") return;
      send("live.peers", {
        session: r.authority.session,
        peers: peerList(r),
        unsynced_files: unsyncedOf(r),
      });
    };

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

    const projectOf = (r: Run): MockProject => {
      if (r.project === null) {
        throw liveFail("not-project", "This is not a project session.");
      }
      return r.project;
    };

    /** Project commands wait for the session to be `live`, like the real driver. */
    const needLive = (r: Run): MockProject => {
      const project = projectOf(r);
      if (project.phase === "manifest") {
        throw liveFail(
          "waiting",
          "Waiting for the host's file list. Try again in a moment.",
        );
      }
      if (project.phase === "plan") {
        throw liveFail("invalid", "Choose what to do with each file first.");
      }
      return project;
    };

    /** The project folder a project command needs (it cannot find it itself). */
    const rootArg = (args: Args): string => {
      const root = args["root"];
      if (typeof root !== "string" || root.trim() === "") {
        throw liveFail(
          "no-project",
          "Open a project folder to use live project sessions.",
        );
      }
      return root;
    };

    /** The document a command names: the file's own, or the project file at `path`. */
    const authorityFor = (r: Run, args: Args): LiveAuthority => {
      if (r.project === null) return r.authority;
      needLive(r);
      const path = typeof args["path"] === "string" ? args["path"] : "";
      const found = r.project.files.get(path);
      if (found === undefined) {
        throw liveFail("not-in-session", `${path || "A file"} is not in the session.`);
      }
      return found;
    };

    const begin = (
      doc: number,
      role: "host" | "guest",
      text: string,
      invite: string,
      file: string | null,
      relay: string,
      project: MockProject | null = null,
    ): Run => {
      const session = `mock${String(nextSession).padStart(4, "0")}`;
      nextSession += 1;
      const peers: MockPeer[] = [];
      const authority = new LiveAuthority(session, text, send, peers);
      if (project === null) authority.onApply = mirror(doc);
      const r: Run = {
        scope: project === null ? "file" : "project",
        authority,
        peers,
        project,
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

    const emptyProject = (): MockProject => ({
      files: new Map(),
      offered: new Map(),
      local: new Map(),
      plan: null,
      attached: new Set(),
      excluded: new Set(),
      deleted: new Map(),
      progress: null,
      following: null,
      phase: "live",
      unsynced: null,
      paused: null,
    });

    const addFile = (r: Run, path: string, text: string): LiveAuthority => {
      const project = projectOf(r);
      const authority = new LiveAuthority(
        r.authority.session,
        text,
        send,
        r.peers,
        path,
      );
      project.files.set(path, authority);
      return authority;
    };

    const text = (doc: number): string =>
      String(host.call("editor.text", { id: doc }) ?? "");

    const filesArg = (value: unknown): Map<string, string> => {
      const out = new Map<string, string>();
      if (typeof value === "object" && value !== null) {
        for (const [path, body] of Object.entries(value)) {
          if (typeof body === "string") out.set(path, body);
        }
      }
      return out;
    };

    const peerAt = (r: Run, id: unknown): MockPeer => {
      const found = r.peers.find((p) => p.peer === String(id));
      if (found === undefined) throw liveFail("no-peer", "There is no such person.");
      return found;
    };

    const inviteFor = (
      relay: string,
      file: string | null,
      scope: LiveScope,
      fragment: string,
    ): string => {
      const url = new URL("manang-live://join");
      url.searchParams.set("relay", relay);
      if (file !== null) url.searchParams.set("file", file);
      if (scope === "project") url.searchParams.set("scope", "project");
      url.hash = fragment;
      return url.toString();
    };

    const startFile = (args: Args): unknown => {
      const relay = normalizeRelay(relayUrl);
      const doc = numberArg(args, "doc");
      const file = typeof args["file"] === "string" ? args["file"] : null;
      const start = text(doc);
      const fragment = `mock${fingerprint(start)}${nextSession.toString(16).padStart(8, "0")}`;
      pretend.set(fragment, start);
      const r = begin(
        doc,
        "host",
        start,
        inviteFor(relay, file, "file", fragment),
        file,
        relay,
      );
      return {
        session: r.authority.session,
        version: 0,
        role: "host",
        invite: r.invite,
      };
    };

    const startProject = (args: Args): unknown => {
      const relay = normalizeRelay(relayUrl);
      rootArg(args);
      const fragment = `mockp${nextSession.toString(16).padStart(8, "0")}${pretendProjects.size}`;
      pretendProjects.set(fragment, new Map(seeded));
      const project = emptyProject();
      const r = begin(
        0,
        "host",
        "",
        inviteFor(relay, null, "project", fragment),
        null,
        relay,
        project,
      );
      for (const [path, body] of seeded) addFile(r, path, body);
      return {
        session: r.authority.session,
        version: 0,
        role: "host",
        scope: "project",
        invite: r.invite,
        files: project.files.size,
      };
    };

    const joinProject = (args: Args, link: ReturnType<typeof parseInvite>): unknown => {
      rootArg(args);
      const offered = pretendProjects.get(link.fragment);
      if (offered === undefined) {
        throw liveFail("bad-invite", "This invite is not valid: nobody is hosting it.");
      }
      const project = emptyProject();
      project.phase = "manifest";
      project.offered = new Map(offered);
      project.local = new Map(localDisk);
      const r = begin(
        0,
        "guest",
        "",
        args["invite"] as string,
        link.file,
        link.relay,
        project,
      );
      r.peers.push({
        peer: String(nextPeer),
        name: "Ada Lovelace",
        slot: 3,
        selection: null,
        path: null,
      });
      nextPeer += 1;
      // Phase 1 ends with the manifest: the shell decides what to do with each file.
      setTimeout(() => {
        if (run !== r || project.plan !== null) return;
        project.plan = classify(project.offered, project.local);
        project.phase = "plan";
        send("live.plan", { session: r.authority.session, files: project.plan });
      }, 20);
      return {
        session: r.authority.session,
        version: 0,
        role: "guest",
        scope: "project",
        file: null,
      };
    };

    const resolve = (args: Args): unknown => {
      const r = current();
      const project = projectOf(r);
      const plan = project.plan;
      if (plan === null) throw liveFail("no-plan", "There is nothing to resolve.");
      const choices = new Map<string, string>();
      for (const choice of Array.isArray(args["choices"])
        ? (args["choices"] as LiveChoice[])
        : []) {
        choices.set(choice.path, choice.action);
      }
      // Never silent: a file that differs needs an answer, and replacing more
      // than one needs an explicit all-files choice (`*`).
      const differing = plan.filter((f) => f.class === "differing");
      const unanswered = differing.filter((f) => !choices.has(f.path));
      if (unanswered.length > 0 && !choices.has("*")) {
        throw liveFail(
          "unresolved",
          `${unanswered.length} file(s) differ and need a choice.`,
        );
      }
      const wholesale = choices.get("*");
      const replacing = differing.filter(
        (f) => (choices.get(f.path) ?? wholesale) !== "keep-mine",
      ).length;
      if (replacing > 1 && wholesale !== "take-host") {
        throw liveFail(
          "needs-confirmation",
          `Replacing ${replacing} of your files needs the all-files confirmation.`,
        );
      }
      const trashed: string[] = [];
      const taken: string[] = [];
      for (const file of plan) {
        const choice = choices.get(file.path) ?? wholesale;
        if (file.class === "identical" || file.class === "missing") {
          taken.push(file.path);
        } else if (file.class === "differing") {
          if (choice === "keep-mine") project.excluded.add(file.path);
          else {
            taken.push(file.path);
            trashed.push(file.path);
          }
        } else if (choice === "share") {
          project.offered.set(file.path, project.local.get(file.path) ?? "");
          taken.push(file.path);
        }
      }
      project.plan = null;
      project.phase = "live";
      project.progress = { done: 0, total: taken.length };
      send("live.progress", { session: r.authority.session, ...project.progress });
      for (const path of taken) {
        addFile(r, path, project.offered.get(path) ?? "");
        project.progress = { done: project.progress.done + 1, total: taken.length };
        send("live.progress", { session: r.authority.session, ...project.progress });
      }
      project.progress = null;
      return { applied: taken.length, trashed };
    };

    const tree = (r: Run, ops: LiveTreeOp[]): void => {
      send("live.tree", { session: r.authority.session, ops });
    };

    const notice = (
      r: Run,
      kind: "joined" | "left" | "deleted" | "renamed",
      peer: string,
      path?: string,
    ): void => {
      if (r.scope !== "project") return;
      send("live.notice", {
        session: r.authority.session,
        kind,
        peer,
        ...(path === undefined ? {} : { path }),
      });
    };

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
          scope: r.scope,
          file: r.file,
          relay: r.relay,
          state: r.state,
          attempt: r.attempt,
          error: r.error,
          version: r.authority.version,
          peers: peerList(r),
          ...(r.project === null
            ? { doc: r.doc }
            : {
                files: r.project.files.size,
                progress: r.project.progress,
                phase: r.project.phase,
                unsynced_files: unsyncedOf(r),
                paused: r.project.paused,
                skipped: [],
                following: r.project.following,
              }),
        };
      },
      "live.start": (args) => {
        if (run !== null) {
          throw liveFail(
            "already-live",
            "A live session is already running. Leave it first.",
          );
        }
        return args["scope"] === "project" ? startProject(args) : startFile(args);
      },
      "live.join": (args) => {
        if (run !== null) {
          throw liveFail(
            "already-live",
            "A live session is already running. Leave it first.",
          );
        }
        const link = parseInvite(
          typeof args["invite"] === "string" ? args["invite"] : "",
        );
        if (link.scope === "project") return joinProject(args, link);
        const doc = numberArg(args, "doc");
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
        r.peers.push({
          peer: String(nextPeer),
          name: "Ada Lovelace",
          slot: 3,
          selection: null,
          path: null,
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
        return { relay: link.relay, file: link.file, scope: link.scope };
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
        return authorityFor(r, args).push(numberArg(args, "version"), changes);
      },
      "live.selection": () => ({}),
      "live.snapshot": (args) => {
        const authority = authorityFor(current(), args);
        return { version: authority.version, text: authority.text };
      },
      "live.contributors": () => {
        const r = current();
        return {
          contributors: r.peers.map((p) => ({ name: p.name, email: null })),
          trailers: [],
          without_email: r.peers.map((p) => p.name),
        };
      },
      "live.resync": (args) => {
        const r = current();
        const project = r.project;
        const answer = args["paused"];
        if (project === null) return { resyncing: true };
        if (answer === undefined) {
          if (project.paused !== null) {
            throw liveFail(
              "paused",
              "A large change on disk is waiting for an answer.",
            );
          }
          return { resyncing: true };
        }
        if (answer !== "share" && answer !== "discard") {
          throw liveFail(
            "invalid",
            `\`paused\` must be "share" or "discard", not ${JSON.stringify(answer)}`,
          );
        }
        project.paused = null;
        return { resyncing: true };
      },

      /* -- Project sessions -------------------------------------------------- */
      "live.resolve": resolve,
      "live.attach": (args) => {
        const r = current();
        const project = needLive(r);
        const path = typeof args["path"] === "string" ? args["path"] : "";
        if (project.excluded.has(path)) {
          throw liveFail("excluded", `${path} is kept out of the session.`);
        }
        const authority = authorityFor(r, args);
        project.attached.add(path);
        return { version: authority.version, text: authority.text };
      },
      "live.detach": (args) => {
        const project = needLive(current());
        const path = typeof args["path"] === "string" ? args["path"] : "";
        return { detached: project.attached.delete(path) };
      },
      "live.follow": (args) => {
        const project = projectOf(current());
        project.following = typeof args["peer"] === "string" ? args["peer"] : null;
        return { following: project.following };
      },
      "live.files": () => {
        const r = current();
        const project = needLive(r);
        const files: LiveFileNode[] = [...project.files.keys()]
          .filter((path) => !project.excluded.has(path))
          .sort()
          .map((path) => ({
            path,
            kind: "text" as const,
            peers: r.peers.filter((p) => p.path === path).map((p) => p.peer),
          }));
        return { files };
      },
      "live.exclude": (args) => {
        const project = needLive(current());
        const path = typeof args["path"] === "string" ? args["path"] : "";
        project.excluded.add(path);
        project.attached.delete(path);
        return { excluded: true };
      },
      "live.include": (args) => {
        const project = needLive(current());
        const path = typeof args["path"] === "string" ? args["path"] : "";
        return { included: project.excluded.delete(path) };
      },
      "live.restore": (args) => {
        const r = current();
        const project = needLive(r);
        const path = typeof args["path"] === "string" ? args["path"] : "";
        const body = project.deleted.get(path);
        if (body === undefined) {
          throw liveFail("not-deleted", `${path} was not deleted in this session.`);
        }
        project.deleted.delete(path);
        addFile(r, path, body);
        tree(r, [{ op: "create", path, by: "self" }]);
        return { restored: true };
      },

      /* -- Dev hooks: the scripted collaborator and the network ------------ */
      "dev.live-seed": (args) => {
        seeded = filesArg(args["files"]);
        return { files: seeded.size };
      },
      "dev.live-local": (args) => {
        localDisk = filesArg(args["files"]);
        return { files: localDisk.size };
      },
      /** What a host would strand by leaving: the files no one present holds. */
      "dev.live-unsynced": (args) => {
        const project = projectOf(current());
        project.unsynced = typeof args["files"] === "number" ? args["files"] : null;
        peersEvent(current());
        return null;
      },
      /** A large rewrite on disk (a Git checkout) is held until it is answered. */
      "dev.live-paused": (args) => {
        const r = current();
        const project = projectOf(r);
        const info: LivePausedInfo = {
          files: typeof args["files"] === "number" ? args["files"] : 40,
          changed_lines:
            typeof args["changed_lines"] === "number" ? args["changed_lines"] : 900,
          total_lines:
            typeof args["total_lines"] === "number" ? args["total_lines"] : 2000,
        };
        project.paused = info;
        send("live.paused", { session: r.authority.session, ...info });
        return null;
      },
      "dev.live-peer-join": (args) => {
        const r = current();
        const peer: MockPeer = {
          peer: String(nextPeer),
          name: typeof args["name"] === "string" ? args["name"] : "Ada Lovelace",
          slot: typeof args["slot"] === "number" ? args["slot"] : 3,
          selection: null,
          path: typeof args["path"] === "string" ? args["path"] : null,
        };
        nextPeer += 1;
        r.peers.push(peer);
        stateEvent(r);
        peersEvent(r);
        notice(r, "joined", peer.peer);
        return { peer: peer.peer };
      },
      "dev.live-peer-leave": (args) => {
        const r = current();
        const at = r.peers.findIndex((p) => p.peer === String(args["peer"]));
        const gone = at >= 0 ? r.peers.splice(at, 1)[0] : undefined;
        stateEvent(r);
        peersEvent(r);
        if (gone !== undefined) notice(r, "left", gone.peer);
        r.authority.caret("", null);
        return null;
      },
      /** A peer brings another file to the front (project sessions). */
      "dev.live-peer-open": (args) => {
        const r = current();
        const peer = peerAt(r, args["peer"]);
        peer.path = typeof args["path"] === "string" ? args["path"] : null;
        peer.selection = null;
        peersEvent(r);
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
        const authority = authorityFor(r, args);
        authority.remote(changes);
        return { version: authority.version };
      },
      "dev.live-peer-caret": (args) => {
        const r = current();
        const authority = authorityFor(r, args);
        const peer = peerAt(r, args["peer"]);
        if (r.project !== null) peer.path = authority.path ?? null;
        authority.caret(String(args["peer"]), {
          anchor: numberArg(args, "anchor"),
          head: numberArg(args, "head"),
        });
        return null;
      },
      "dev.live-peer-create": (args) => {
        const r = current();
        const project = projectOf(r);
        const peer = peerAt(r, args["peer"]);
        const path = typeof args["path"] === "string" ? args["path"] : "";
        if (unsafePath(path)) throw liveFail("bad-path", `${path} is not a safe path.`);
        addFile(r, path, typeof args["text"] === "string" ? args["text"] : "");
        project.excluded.delete(path);
        tree(r, [{ op: "create", path, by: peer.peer }]);
        return null;
      },
      "dev.live-peer-delete": (args) => {
        const r = current();
        const project = projectOf(r);
        const peer = peerAt(r, args["peer"]);
        const path = typeof args["path"] === "string" ? args["path"] : "";
        const authority = project.files.get(path);
        if (authority === undefined) return null;
        project.files.delete(path);
        project.attached.delete(path);
        project.deleted.set(path, authority.text);
        // `anonymous`: the session cannot tell who deleted it (`by` is empty).
        tree(r, [
          { op: "delete", path, by: args["anonymous"] === true ? "" : peer.peer },
        ]);
        notice(r, "deleted", peer.peer, path);
        return null;
      },
      "dev.live-peer-rename": (args) => {
        const r = current();
        const project = projectOf(r);
        const peer = peerAt(r, args["peer"]);
        const from = typeof args["from"] === "string" ? args["from"] : "";
        const to = typeof args["to"] === "string" ? args["to"] : "";
        const authority = project.files.get(from);
        if (authority === undefined) return null;
        if (unsafePath(to)) throw liveFail("bad-path", `${to} is not a safe path.`);
        project.files.delete(from);
        const moved = addFile(r, to, authority.text);
        moved.version = authority.version;
        project.attached.delete(from);
        tree(r, [{ op: "rename", path: to, from, by: peer.peer }]);
        notice(r, "renamed", peer.peer, to);
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
      /** An invite for a pretend host whose file holds `text` (or whose project holds `files`). */
      "dev.live-make-invite": (args) => {
        const relay = "wss://relay.example.org";
        if (typeof args["files"] === "object" && args["files"] !== null) {
          const fragment = `mockp${(2000 + pretendProjects.size).toString(16).padStart(8, "0")}${pretendProjects.size}`;
          pretendProjects.set(fragment, filesArg(args["files"]));
          return { invite: inviteFor(relay, null, "project", fragment) };
        }
        const start = typeof args["text"] === "string" ? args["text"] : "";
        const fragment = `mock${fingerprint(start)}${(1000 + pretend.size).toString(16).padStart(8, "0")}`;
        pretend.set(fragment, start);
        return {
          invite: inviteFor(
            relay,
            typeof args["file"] === "string" ? args["file"] : null,
            "file",
            fragment,
          ),
        };
      },
      "dev.live-text": (args) => {
        if (run === null) return null;
        return run.project === null
          ? run.authority.text
          : (run.project.files.get(String(args["path"] ?? ""))?.text ?? null);
      },
    };

    return {
      name: "live",
      commands: COMMANDS,
      handlers,
    };
  }
};
