/**
 * Pure wording and rules for the Share and Get updates group of the Versions
 * view. No kernel or UI here, so they are unit-tested. Plain language only:
 * Share and Get updates, never push and pull.
 */

import type { GitSyncStatus, GitUpdated } from "$lib/kernel/api";

/** Which single action the status suggests (it alone is drawn lime). */
export type ShareSuggestion = "share" | "get-updates" | "none";

export function shareSuggestion(status: GitSyncStatus | null): ShareSuggestion {
  if (status === null) return "none";
  switch (status.state) {
    case "ahead":
      return "share";
    case "behind":
    case "diverged":
      return "get-updates";
    default:
      return "none";
  }
}

function plural(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

/** "2 changes to share", "Up to date", "Updates available". */
export function statusLine(status: GitSyncStatus | null): string {
  if (status === null) return "";
  switch (status.state) {
    case "no-remote":
      return "Not shared yet";
    case "up-to-date":
      return "Up to date";
    case "ahead":
      return status.never_shared
        ? `Not shared yet: ${plural(status.ahead, "version", "versions")} to share`
        : `${plural(status.ahead, "change", "changes")} to share`;
    case "behind":
      return "Updates available";
    case "diverged":
      return `Updates available, and ${plural(status.ahead, "change", "changes")} to share`;
  }
}

/** A hint under the status when unsaved edits are not part of what is shared. */
export function unsavedHint(status: GitSyncStatus | null): string {
  if (status === null || status.uncommitted === 0) return "";
  const n = status.uncommitted;
  return `${plural(n, "file has", "files have")} unsaved changes. Save a version to include ${n === 1 ? "it" : "them"} when sharing.`;
}

/** The notice after a successful Share. */
export function sharedNotice(pushed: number): string {
  return pushed > 0
    ? `Shared ${plural(pushed, "change", "changes")}`
    : "Everything is already shared";
}

export interface UpdateWords {
  readonly kind: "notice" | "conflicts";
  readonly text: string;
}

/** What to tell the person after Get updates. */
export function updatedWords(result: GitUpdated): UpdateWords {
  switch (result.result) {
    case "up-to-date":
      return { kind: "notice", text: "You already have the latest" };
    case "fast-forwarded":
    case "merged": {
      const saved =
        result.safety_version !== null
          ? " Your unsaved changes were kept as a version first."
          : "";
      return {
        kind: "notice",
        text: `Got ${plural(result.received, "update", "updates")}.${saved}`,
      };
    }
    case "diverged":
      return {
        kind: "notice",
        text: "Your copy and the shared copy both changed. Nothing was changed here.",
      };
    case "conflicts": {
      const list = result.files.join(", ");
      return {
        kind: "conflicts",
        text: `You and someone else both changed ${plural(result.files.length, "file", "files")}: ${list}. Nothing was changed on your computer. Ask them to share first, or copy your edits aside and try again.`,
      };
    }
  }
}

/** A friendly check on the address before it reaches git. */
export function addressProblem(url: string): string {
  const text = url.trim();
  if (text === "") return "Paste the address of the shared project first.";
  if (/\s/.test(text)) return "An address has no spaces in it.";
  return "";
}
