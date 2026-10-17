/**
 * Git workbench models and pure utility functions.
 */

import type { GitStatusFile } from "$lib/kernel/api";
import { errorMessage } from "$lib/kernel";

export interface GitGroups {
  readonly staged: readonly GitStatusFile[];
  readonly changes: readonly GitStatusFile[];
  readonly untracked: readonly GitStatusFile[];
  readonly conflicted: readonly GitStatusFile[];
}

/**
 * Group files from GitStatusReport into staged, changes, untracked, and conflicts.
 */
export function groupGitFiles(files: readonly GitStatusFile[]): GitGroups {
  const staged: GitStatusFile[] = [];
  const changes: GitStatusFile[] = [];
  const untracked: GitStatusFile[] = [];
  const conflicted: GitStatusFile[] = [];

  for (const f of files) {
    if (f.conflicted) {
      conflicted.push(f);
      continue;
    }
    if (f.untracked) {
      untracked.push(f);
      continue;
    }
    if (f.staged !== null) {
      staged.push(f);
    }
    if (f.unstaged !== null) {
      changes.push(f);
    }
  }

  return { staged, changes, untracked, conflicted };
}

/**
 * Format a Unix epoch timestamp (seconds) into a readable relative time string.
 */
export function formatRelativeTime(epochSecs: number, nowSecs = Math.floor(Date.now() / 1000)): string {
  const diff = Math.max(0, nowSecs - epochSecs);
  if (diff < 60) return "just now";
  if (diff < 3600) {
    const mins = Math.floor(diff / 60);
    return `${mins}m ago`;
  }
  if (diff < 86400) {
    const hours = Math.floor(diff / 3600);
    return `${hours}h ago`;
  }
  const days = Math.floor(diff / 86400);
  if (days === 1) return "yesterday";
  if (days < 30) return `${days}d ago`;
  return new Date(epochSecs * 1000).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

/**
 * Parse structured JSON GitError messages from kernel invocation failures.
 */
export function parseGitError(error: unknown): { code: string; message: string } {
  const raw = errorMessage(error);
  try {
    const parsed = JSON.parse(raw);
    if (
      typeof parsed === "object" &&
      parsed !== null &&
      typeof parsed.code === "string" &&
      typeof parsed.message === "string"
    ) {
      return { code: parsed.code, message: parsed.message };
    }
  } catch {
    // raw string
  }
  return { code: "unknown", message: raw };
}

/**
 * Return actionable guidance for specific Git error codes.
 */
export function gitActionableError(code: string, message: string): string {
  switch (code) {
    case "missing-identity":
      return "Git author identity is missing. Run `git config --global user.name \"Your Name\"` and `git config --global user.email you@example.com` in your terminal, then try again.";
    case "nothing-to-commit":
      return "Nothing staged to commit. Stage your changes with `+` before committing.";
    case "empty-message":
      return "Commit message cannot be empty. Please enter a commit message.";
    case "confirm-required":
      return "Confirmation is required to discard changes permanently.";
    case "dirty-tree":
      return "Working directory has uncommitted changes. Stage, commit, or stash them first.";
    case "not-a-repo":
      return "This folder is not a Git repository. Click 'Initialize repository' to start tracking changes.";
    default:
      return message;
  }
}

/**
 * Map status state string to short badge letter and semantic color class.
 */
export function statusBadge(state: string | null, isUntracked = false): { label: string; textClass: string } {
  if (isUntracked) return { label: "U", textClass: "text-text-60" };
  switch (state) {
    case "added":
      return { label: "A", textClass: "text-accent" };
    case "modified":
      return { label: "M", textClass: "text-accent" };
    case "deleted":
      return { label: "D", textClass: "text-text-muted" };
    case "renamed":
      return { label: "R", textClass: "text-accent" };
    case "conflicted":
      return { label: "C", textClass: "text-red-400" };
    default:
      return { label: "?", textClass: "text-text-muted" };
  }
}
