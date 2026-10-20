/**
 * Typed access to the `live` plugin (`crates/manang-live-plugin`).
 *
 * The Rust side owns the CRDT, the encryption and the relay connection. The
 * shell only pushes local edits and the caret, and receives `live.remote`,
 * `live.presence` and `live.state` events on the kernel bus. A project session
 * adds `live.plan`, `live.tree`, `live.peers`, `live.progress` and
 * `live.notice`, and names the file in every push, caret and remote step. See
 * `src/lib/editor/live/binding.ts` for the edit protocol.
 */

import { errorMessage, type Kernel } from "./client";

/** One replacement in UTF-16 units, sequential within a batch. */
export interface LiveChange {
  readonly start: number;
  readonly delete: number;
  readonly insert: string;
}

export type LiveConnection =
  "connecting" | "waiting" | "connected" | "reconnecting" | "left" | "error";

/** `file`: one text file (the original sessions); `project`: the whole tree. */
export type LiveScope = "file" | "project";

export interface LivePeer {
  /** Decimal string: ids are 64-bit. */
  readonly peer: string;
  readonly name: string;
  /** Colour slot, `0..7`. */
  readonly slot: number;
}

/** A peer and the file they have in front of them (`null`: none or unknown). */
export interface LivePeerAt extends LivePeer {
  readonly path?: string | null;
}

export interface LiveRemoteSelection extends LivePeer {
  readonly anchor: number;
  readonly head: number;
}

export interface LiveStatus {
  readonly active: boolean;
  /** Whether Settings has a relay address (sharing needs one; joining does not). */
  readonly relay_set: boolean;
  readonly session?: string;
  readonly role?: "host" | "guest";
  readonly doc?: number;
  readonly file?: string | null;
  readonly relay?: string;
  readonly state?: LiveConnection;
  readonly attempt?: number;
  readonly error?: string | null;
  readonly version?: number;
  readonly peers?: readonly LivePeerAt[];
  readonly scope?: LiveScope;
  /** Files in a project session. */
  readonly files?: number;
  readonly progress?: LiveProgress | null;
}

export interface LiveProgress {
  readonly done: number;
  readonly total: number;
}

export interface LiveStarted {
  readonly session: string;
  /** Absent from a plugin that predates project sessions: it means `file`. */
  readonly scope?: LiveScope;
  readonly version: number;
  readonly role: "host" | "guest";
  readonly invite?: string;
  readonly file?: string | null;
  /**
   * A joiner waits for someone in the room to answer: the file keeps its own
   * text (read-only) until then, and is replaced by the room's only after.
   */
  readonly waiting?: boolean;
  /** Unused since a join no longer blanks the file up front. */
  readonly reset?: LiveChange | null;
}

/** How a file of the host's project compares with the guest's disk. */
export type LivePlanClass = "identical" | "missing" | "differing" | "guest-only";

export interface LivePlanFile {
  readonly path: string;
  readonly class: LivePlanClass;
}

/** What the guest decides for one file of the plan. */
export type LiveChoiceAction = "take-host" | "keep-mine" | "share" | "keep-private";

export interface LiveChoice {
  readonly path: string;
  readonly action: LiveChoiceAction;
}

/** A file of the session tree. */
export interface LiveFileNode {
  readonly path: string;
  readonly kind: "text" | "binary";
  /** Peers that have the file open. */
  readonly peers: readonly string[];
}

/** The `live.remote` event: a step of the session, in order. */
export interface LiveRemoteEvent {
  readonly session: string;
  /** The file (always present from a project-capable plugin). */
  readonly path?: string;
  readonly version: number;
  /** True when it only acknowledges the view's own pushed edits. */
  readonly ack: boolean;
  readonly changes: readonly LiveChange[];
}

export interface LivePresenceEvent {
  readonly session: string;
  readonly path?: string;
  readonly version: number;
  readonly selections: readonly LiveRemoteSelection[];
}

export interface LiveStateEvent {
  readonly session: string;
  readonly state: LiveConnection;
  readonly attempt: number;
  readonly error: string | null;
  readonly peers: readonly LivePeerAt[];
}

/** `live.plan`: the guest's files classified against the host's manifest. */
export interface LivePlanEvent {
  readonly session: string;
  readonly files: readonly LivePlanFile[];
}

export interface LiveTreeOp {
  readonly op: "create" | "delete" | "rename" | "move";
  readonly path: string;
  readonly from?: string;
  /** Peer id of the author. */
  readonly by: string;
}

export interface LiveTreeEvent {
  readonly session: string;
  readonly ops: readonly LiveTreeOp[];
}

export interface LivePeersEvent {
  readonly session: string;
  readonly peers: readonly LivePeerAt[];
}

export interface LiveProgressEvent extends LiveProgress {
  readonly session: string;
}

export interface LiveNoticeEvent {
  readonly session: string;
  readonly kind: "joined" | "left" | "deleted" | "renamed";
  readonly peer: string;
  readonly path?: string;
}

export interface LivePushResult {
  readonly accepted: boolean;
  readonly version: number;
}

/** What a `live.*` command refused with: a stable code and a sentence. */
export interface LiveError {
  readonly code: string;
  readonly message: string;
}

/** Decode a refused `live.*` command (the plugin's errors are `{code, message}` JSON). */
export function liveError(error: unknown): LiveError {
  const text = errorMessage(error);
  try {
    const parsed: unknown = JSON.parse(text);
    if (
      typeof parsed === "object" &&
      parsed !== null &&
      typeof (parsed as { code?: unknown }).code === "string" &&
      typeof (parsed as { message?: unknown }).message === "string"
    ) {
      return parsed as LiveError;
    }
  } catch {
    /* not JSON: a kernel-level failure */
  }
  return { code: "kernel", message: text };
}

export function liveApi(kernel: Kernel) {
  return {
    status: () => kernel.run<LiveStatus>("live.status"),
    start: (args: { doc?: number; file?: string; scope?: LiveScope }) =>
      kernel.run<LiveStarted>("live.start", args),
    /** Phase 1 of a project join: connect and fetch the manifest (a `live.plan` follows). */
    join: (args: {
      doc?: number;
      invite: string;
      name?: string;
      resolution?: "take-host";
    }) => kernel.run<LiveStarted>("live.join", args),
    leave: () => kernel.run<{ left: boolean }>("live.leave"),
    invite: () => kernel.run<{ invite: string; file: string | null }>("live.invite"),
    inspect: (invite: string) =>
      kernel.run<{ relay: string; file: string | null; scope?: LiveScope }>(
        "live.inspect",
        { invite },
      ),
    push: (args: {
      session: string;
      path: string;
      version: number;
      changes: readonly LiveChange[];
    }) => kernel.run<LivePushResult>("live.push", args),
    selection: (args: {
      session: string;
      path: string;
      version: number;
      anchor?: number;
      head?: number;
    }) => kernel.run("live.selection", args),
    snapshot: (session: string, path: string) =>
      kernel.run<{ version: number; text: string }>("live.snapshot", {
        session,
        path,
      }),
    /** Phase 2 of a project join: apply the choices made on the plan. */
    resolve: (choices: readonly LiveChoice[]) =>
      kernel.run<{ applied: number; trashed: readonly string[] }>("live.resolve", {
        choices,
      }),
    /** Start editing a file of the session: its text and the version to push against. */
    attach: (path: string) =>
      kernel.run<{ version: number; text: string }>("live.attach", { path }),
    detach: (path: string) =>
      kernel.run<{ detached: boolean }>("live.detach", { path }),
    /** Local only: which peer this window follows (`null` stops). */
    follow: (peer: string | null) =>
      kernel.run<{ following: string | null }>("live.follow", { peer }),
    files: () => kernel.run<{ files: readonly LiveFileNode[] }>("live.files"),
    exclude: (path: string) =>
      kernel.run<{ excluded: boolean }>("live.exclude", { path }),
    include: (path: string) =>
      kernel.run<{ included: boolean }>("live.include", { path }),
    restore: (path: string) =>
      kernel.run<{ restored: boolean }>("live.restore", { path }),
    resync: () => kernel.run<{ resyncing: boolean }>("live.resync"),
    /** Who edited since the last call, with ready-made commit trailers. */
    contributors: (committer: { name?: string | null; email?: string | null } = {}) =>
      kernel.run<{
        contributors: readonly { name: string; email: string | null }[];
        trailers: readonly string[];
        without_email: readonly string[];
      }>("live.contributors", {
        committer_name: committer.name ?? undefined,
        committer_email: committer.email ?? undefined,
      }),
    configure: (values: Record<string, unknown>) =>
      kernel.run<{ applied: number }>("live.configure", { values }),
  };
}

export type LiveApi = ReturnType<typeof liveApi>;
