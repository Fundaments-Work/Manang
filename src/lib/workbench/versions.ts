/**
 * Pure rules behind "Save a version" and the Versions panel: counting what
 * changed, naming a version, which files a restore would overwrite, and the
 * wording of its confirmation. No UI or kernel here, so they are unit-tested.
 */

import type { GitFileDiff, GitStatusFile, GitVersionInfo } from "$lib/kernel/api";

/** Which of the Source Control view's two modes is showing. */
export type ScmMode = "changes" | "versions";

/** Files with any uncommitted change (conflicts included; they are changes too). */
export function changedPaths(files: readonly GitStatusFile[]): string[] {
  return files
    .filter(
      (f) => f.staged !== null || f.unstaged !== null || f.untracked || f.conflicted,
    )
    .map((f) => f.path);
}

/** "3 files have changed since the last version." */
export function changeSummary(count: number): string {
  if (count === 0) return "Nothing has changed since the last version.";
  return `${count} ${count === 1 ? "file has" : "files have"} changed since the last version.`;
}

/** The text a version is shown under: its name, or the commit subject. */
export function versionTitle(version: Pick<GitVersionInfo, "subject">): string {
  const subject = version.subject.trim();
  return subject === "" ? "(no description)" : subject;
}

/** "3 files", "1 file". */
export function fileCount(count: number): string {
  return `${count} ${count === 1 ? "file" : "files"}`;
}

export interface RestoreScope {
  /** `"whole"` restores every file the version has; `"file"` just `paths`. */
  readonly kind: "file" | "whole";
  /** For a whole restore, the files the version has. */
  readonly paths: readonly string[];
}

export interface RestorePlan {
  /** Files with uncommitted changes that the restore would replace. */
  readonly dirty: readonly string[];
  /** Offer, and default on, "Save my changes first". */
  readonly saveFirst: boolean;
}

/**
 * Which current changes a restore of `scope` would overwrite. A whole-project
 * restore cannot know which files differ without asking the kernel, so any
 * change at all counts as at risk; a file restore is exact.
 */
export function planRestore(
  scope: RestoreScope,
  changed: readonly string[],
): RestorePlan {
  const at = new Set(changed);
  const dirty =
    scope.kind === "whole" ? [...changed] : scope.paths.filter((p) => at.has(p));
  return { dirty, saveFirst: dirty.length > 0 };
}

export interface RestoreWords {
  readonly title: string;
  readonly message: string;
  readonly action: string;
}

/** The confirmation's wording: plain about what happens and what does not. */
export function restoreWords(
  scope: RestoreScope,
  version: Pick<GitVersionInfo, "short" | "subject">,
  plan: RestorePlan,
): RestoreWords {
  const what =
    scope.kind === "whole"
      ? "Every file in this version"
      : scope.paths.length === 1
        ? (scope.paths[0] ?? "this file")
        : `${scope.paths.length} files`;
  const saved = plan.saveFirst
    ? `Your current changes to these files are saved first, as a version named "Before restoring ${version.short}". `
    : "";
  return {
    title: scope.kind === "whole" ? "Restore the whole project?" : "Restore this file?",
    message: `${what} will be replaced by ${
      scope.kind === "file" && scope.paths.length === 1 ? "its" : "their"
    } content from "${version.subject}". ${saved}Nothing is deleted, and no version is lost.`,
    action: plan.saveFirst ? "Save my changes first, then restore" : "Restore",
  };
}

/** The notice after a restore. */
export function restoredNotice(
  restored: readonly string[],
  untouchedNew: readonly string[],
): string {
  if (restored.length === 0) return "Nothing to restore: those files already match.";
  const head =
    restored.length === 1
      ? `Restored ${restored[0] ?? ""}`
      : `Restored ${restored.length} files`;
  return untouchedNew.length === 0
    ? `${head}.`
    : `${head}. ${fileCount(untouchedNew.length)} added since then ${untouchedNew.length === 1 ? "was" : "were"} left as they are.`;
}

/** Sort a version's changed files by folder then name, for a stable list. */
export function sortedFiles(files: readonly GitFileDiff[]): GitFileDiff[] {
  return [...files].sort((a, b) => a.path.localeCompare(b.path));
}

/** The default name offered when a person leaves the field empty (local time). */
export function defaultVersionName(now: Date): string {
  const p = (n: number): string => String(n).padStart(2, "0");
  return `Version of ${now.getFullYear()}-${p(now.getMonth() + 1)}-${p(now.getDate())} ${p(now.getHours())}:${p(now.getMinutes())}`;
}
