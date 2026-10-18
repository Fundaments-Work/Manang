/**
 * Plain rules of the live-session UI: what each state says, and how people are
 * listed. Kept apart from the store so they are tested without a kernel.
 */

import type { LiveConnection, LivePeer } from "$lib/kernel/liveApi";

/** What the session bar says for each connection state. */
export function connectionLabel(state: LiveConnection, attempt: number): string {
  switch (state) {
    case "connecting":
      return "Connecting";
    case "connected":
      return "Live";
    case "reconnecting":
      return attempt > 1 ? `Reconnecting (attempt ${attempt})` : "Reconnecting";
    case "left":
      return "Left the session";
    case "error":
      return "Session stopped";
  }
}

/**
 * A sentence for the states that need one; `null` when the label is enough.
 * Edits made while reconnecting are kept and merge afterwards: say so.
 */
export function connectionDetail(
  state: LiveConnection,
  error: string | null,
): string | null {
  switch (state) {
    case "reconnecting":
      return "Your edits are kept and will merge when the connection is back.";
    case "error":
      return error ?? "The session failed.";
    default:
      return null;
  }
}

/** Whether the document can be typed into while the session is in this state. */
export function editingAllowed(state: LiveConnection): boolean {
  return state !== "error";
}

/** The relay is not set: sharing (not joining) needs one. */
export const NO_RELAY_MESSAGE = "Set a relay in Settings to share live.";

/** The names shown for a session, you first. Names are claims, not logins. */
export function participantNames(peers: readonly LivePeer[]): string[] {
  return ["You", ...peers.map((p) => p.name)];
}

/** "You and Ada", "You, Ada and Grace", "You". */
export function participantSummary(peers: readonly LivePeer[]): string {
  const names = participantNames(peers);
  if (names.length <= 1) return "Only you so far";
  const last = names[names.length - 1] ?? "";
  return `${names.slice(0, -1).join(", ")} and ${last}`;
}

/** How many people are in the session, you included. */
export function peopleCount(peers: readonly LivePeer[]): number {
  return peers.length + 1;
}

/**
 * The status-bar chip while a session exists: "Live: 2 people". A connection
 * that is not up says so in the same words as the session bar.
 */
export function liveChipText(
  state: LiveConnection,
  peers: readonly LivePeer[],
): string {
  switch (state) {
    case "connecting":
      return "Live: connecting";
    case "reconnecting":
      return "Live: reconnecting";
    case "error":
      return "Live: stopped";
    case "left":
      return "Live: left";
    case "connected": {
      const n = peopleCount(peers);
      return n === 1 ? "Live: just you" : `Live: ${n} people`;
    }
  }
}

/** One or two capital letters for a person's mark: "Ada Lovelace" is "AL". */
export function initials(name: string): string {
  const words = name
    .trim()
    .split(/\s+/)
    .filter((w) => w !== "");
  const first = [...(words[0] ?? "")][0];
  if (first === undefined) return "?";
  const last = words.length > 1 ? [...(words[words.length - 1] ?? "")][0] : undefined;
  return (first + (last ?? "")).toUpperCase();
}

/** Text files only: a live session edits one text document. */
export function canShareLive(
  kind: "document" | "figure" | "viewer" | undefined,
): boolean {
  return kind === "document";
}

/** The file name of a path, for the notice a join shows. */
export function inviteFileLabel(file: string | null | undefined): string {
  if (file === null || file === undefined || file === "") return "the shared file";
  return file.split("/").pop() ?? file;
}
