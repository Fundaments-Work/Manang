/**
 * Typed access to the `live` plugin (`crates/manang-live-plugin`).
 *
 * The Rust side owns the CRDT, the encryption and the relay connection. The
 * shell only pushes local edits and the caret, and receives `live.remote`,
 * `live.presence` and `live.state` events on the kernel bus. See
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
  "connecting" | "connected" | "reconnecting" | "left" | "error";

export interface LivePeer {
  /** Decimal string: ids are 64-bit. */
  readonly peer: string;
  readonly name: string;
  /** Colour slot, `0..7`. */
  readonly slot: number;
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
  readonly peers?: readonly LivePeer[];
}

export interface LiveStarted {
  readonly session: string;
  readonly version: number;
  readonly role: "host" | "guest";
  readonly invite?: string;
  readonly file?: string | null;
  /** The joiner's copy was replaced by the host's: apply this to the view first. */
  readonly reset?: LiveChange | null;
}

/** The `live.remote` event: a step of the session, in order. */
export interface LiveRemoteEvent {
  readonly session: string;
  readonly version: number;
  /** True when it only acknowledges the view's own pushed edits. */
  readonly ack: boolean;
  readonly changes: readonly LiveChange[];
}

export interface LivePresenceEvent {
  readonly session: string;
  readonly version: number;
  readonly selections: readonly LiveRemoteSelection[];
}

export interface LiveStateEvent {
  readonly session: string;
  readonly state: LiveConnection;
  readonly attempt: number;
  readonly error: string | null;
  readonly peers: readonly LivePeer[];
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
    start: (args: { doc: number; file?: string }) =>
      kernel.run<LiveStarted>("live.start", args),
    join: (args: {
      doc: number;
      invite: string;
      name?: string;
      resolution?: "take-host";
    }) => kernel.run<LiveStarted>("live.join", args),
    leave: () => kernel.run<{ left: boolean }>("live.leave"),
    invite: () => kernel.run<{ invite: string; file: string | null }>("live.invite"),
    inspect: (invite: string) =>
      kernel.run<{ relay: string; file: string | null }>("live.inspect", { invite }),
    push: (args: {
      session: string;
      version: number;
      changes: readonly LiveChange[];
    }) => kernel.run<LivePushResult>("live.push", args),
    selection: (args: {
      session: string;
      version: number;
      anchor?: number;
      head?: number;
    }) => kernel.run("live.selection", args),
    snapshot: (session: string) =>
      kernel.run<{ version: number; text: string }>("live.snapshot", { session }),
    contributors: () =>
      kernel.run<{ contributors: readonly { name: string; email: string | null }[] }>(
        "live.contributors",
      ),
    configure: (values: Record<string, unknown>) =>
      kernel.run<{ applied: number }>("live.configure", { values }),
  };
}

export type LiveApi = ReturnType<typeof liveApi>;
