/**
 * The words of the recovery dialog: plain functions of the snapshot list, so
 * the wording is tested and the dialog stays a thin presenter.
 */

import type { RecoveryEntry } from "$lib/kernel/api";

import { basename } from "./paths";

/** "14:32" for today, "12 Oct, 14:32" for another day. */
export function whenLabel(ms: number, now: number): string {
  const at = new Date(ms);
  const today = new Date(now);
  const time = at.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const sameDay =
    at.getFullYear() === today.getFullYear() &&
    at.getMonth() === today.getMonth() &&
    at.getDate() === today.getDate();
  if (sameDay) return time;
  const day = at.toLocaleDateString([], { day: "numeric", month: "short" });
  return `${day}, ${time}`;
}

/** The newest snapshot's time: when the edits stopped. */
export function latest(entries: readonly RecoveryEntry[]): number {
  return entries.reduce((max, e) => Math.max(max, e.saved_at), 0);
}

export function recoveryTitle(entries: readonly RecoveryEntry[], now: number): string {
  const n = entries.length;
  return `Recover unsaved changes from ${whenLabel(latest(entries), now)} in ${n} ${
    n === 1 ? "file" : "files"
  }?`;
}

const SHOWN_NAMES = 4;

export function recoveryMessage(entries: readonly RecoveryEntry[]): string {
  const names = entries.slice(0, SHOWN_NAMES).map((e) => basename(e.path));
  const more = entries.length - names.length;
  const list = `${names.join(", ")}${more > 0 ? ` and ${more} more` : ""}`;
  const changed = entries.filter((e) => e.disk === "changed").length;
  const missing = entries.filter((e) => e.disk === "missing").length;
  const parts = [
    `Manang found unsaved edits in ${list}, left behind when it last closed. Recovering opens them as unsaved changes; nothing on disk is replaced until you save.`,
  ];
  if (changed > 0) {
    parts.push(
      `${changed === 1 ? "One file has" : `${changed} files have`} also changed on disk since: review shows the difference.`,
    );
  }
  if (missing > 0) {
    parts.push(
      `${missing === 1 ? "One file is" : `${missing} files are`} not on disk any more or never were saved.`,
    );
  }
  return parts.join(" ");
}
