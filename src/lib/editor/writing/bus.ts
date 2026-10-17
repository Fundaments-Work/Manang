/**
 * How the editor extensions hand results to the rest of the app (the Problems
 * panel, the status bar) without importing the app's state: a tiny publisher.
 * The Svelte store in `state/writing.svelte.ts` subscribes once.
 */

import type { EditorView } from "@codemirror/view";

import type { TextStats } from "$lib/kernel/api";

export type FindingSeverity = "error" | "warning" | "info";

/** One prose-lint finding, located in the editor's own UTF-16 positions. */
export interface LintFinding {
  readonly from: number;
  readonly to: number;
  readonly severity: FindingSeverity;
  readonly message: string;
  readonly code: string;
  readonly hints: readonly string[];
}

/** What a format request ended with, in words fit for the status line. */
export interface FormatReport {
  readonly kind:
    "formatted" | "unchanged" | "unsupported" | "skipped" | "stale" | "failed";
  readonly message: string;
}

export interface WritingEvents {
  format(view: EditorView, report: FormatReport): void;
  lint(view: EditorView, findings: readonly LintFinding[]): void;
  stats(view: EditorView, stats: TextStats): void;
}

const listeners = new Set<Partial<WritingEvents>>();

export function subscribeWriting(listener: Partial<WritingEvents>): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function emitLint(view: EditorView, findings: readonly LintFinding[]): void {
  for (const l of listeners) l.lint?.(view, findings);
}

export function emitFormat(view: EditorView, report: FormatReport): void {
  for (const l of listeners) l.format?.(view, report);
}

export function emitStats(view: EditorView, stats: TextStats): void {
  for (const l of listeners) l.stats?.(view, stats);
}
