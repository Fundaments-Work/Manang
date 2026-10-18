/**
 * In-memory `git` plugin.
 *
 * Reproduces `crates/manang-git` command surface (22 commands)
 * over `MockProjectHost` in browser and tests.
 */

import { defaultVersionName } from "$lib/workbench/versions";
import type { MockPlugin, MockPluginFactory } from "./mock";
import type { MockProjectFs, MockProjectHost } from "./mockProject";
import type {
  GitBranchInfo,
  GitCommitInfo,
  GitDiffHunk,
  GitDiffLine,
  GitDiffResponse,
  GitFileDiff,
  GitIdentity,
  GitLogResponse,
  GitRestored,
  GitSavedVersion,
  GitVersionInfo,
  GitVersionsPage,
  GitStatusFile,
  GitStatusReport,
} from "./api";

const COMMANDS: readonly (readonly [string, string])[] = [
  ["git.blame", "Blame file"],
  ["git.branches", "List branches"],
  ["git.checkout-branch", "Switch or create branch"],
  ["git.commit", "Commit staged changes"],
  ["git.diff", "Structured diff hunks"],
  ["git.diff-text", "Unified diff text"],
  ["git.discard", "Discard changes (recoverable)"],
  ["git.identity-get", "Read the author name and email"],
  ["git.identity-set", "Set the author name and email"],
  ["git.init", "Initialise repository"],
  ["git.log", "Commit log"],
  ["git.open", "Set the git project root"],
  ["git.restore", "Restore files from a version"],
  ["git.save-version", "Save a version"],
  ["git.show", "File content at revision"],
  ["git.stage", "Stage files"],
  ["git.status", "Git status"],
  ["git.sync", "Fetch, pull or push"],
  ["git.unstage", "Unstage files"],
  ["git.version-diff", "One file's diff for a version"],
  ["git.version-files", "Files changed by a version"],
  ["git.versions", "Version timeline"],
];

/** The dev kernel's switch for "no author identity yet" (`localStorage`). */
export const MOCK_IDENTITY_FLAG = "manang.mock.git-identity";

function gitFail(code: string, message: string, extra: Record<string, unknown> = {}): never {
  throw { message: JSON.stringify({ code, message, ...extra }) };
}

function identityFlagIsNone(): boolean {
  try {
    return (
      typeof localStorage !== "undefined" &&
      localStorage.getItem(MOCK_IDENTITY_FLAG) === "none"
    );
  } catch {
    return false;
  }
}

function buildFileDiff(
  path: string,
  oldContent: string | undefined,
  newContent: string | undefined,
): GitFileDiff {
  const oldLines = oldContent ? oldContent.split("\n") : [];
  const newLines = newContent ? newContent.split("\n") : [];
  const lines: GitDiffLine[] = [];
  let additions = 0;
  let deletions = 0;
  const max = Math.max(oldLines.length, newLines.length);
  for (let i = 0; i < max; i += 1) {
    const oldL = oldLines[i];
    const newL = newLines[i];
    if (oldL === newL) {
      if (oldL !== undefined) {
        lines.push({
          kind: "context",
          text: oldL,
          old_line: i + 1,
          new_line: i + 1,
          old_offset: 0,
          new_offset: 0,
          no_newline: false,
        });
      }
    } else {
      if (oldL !== undefined) {
        deletions += 1;
        lines.push({
          kind: "remove",
          text: oldL,
          old_line: i + 1,
          new_line: null,
          old_offset: 0,
          new_offset: null,
          no_newline: false,
        });
      }
      if (newL !== undefined) {
        additions += 1;
        lines.push({
          kind: "add",
          text: newL,
          old_line: null,
          new_line: i + 1,
          old_offset: null,
          new_offset: 0,
          no_newline: false,
        });
      }
    }
  }
  const hunk: GitDiffHunk = {
    old_start: 1,
    old_lines: oldLines.length,
    new_start: 1,
    new_lines: newLines.length,
    header: `@@ -1,${oldLines.length} +1,${newLines.length} @@`,
    lines,
  };
  return {
    path,
    status:
      oldContent === undefined ? "added" : newContent === undefined ? "deleted" : "modified",
    binary: false,
    too_large: false,
    additions,
    deletions,
    hunks: [hunk],
  };
}

function str(args: Record<string, unknown>, key: string): string {
  const value = args[key];
  if (typeof value !== "string") {
    throw { message: JSON.stringify({ code: "bad-arguments", message: `\`${key}\` must be a string` }) };
  }
  return value;
}

function optStr(args: Record<string, unknown>, key: string): string | undefined {
  const value = args[key];
  return typeof value === "string" ? value : undefined;
}

function optBool(args: Record<string, unknown>, key: string): boolean {
  const value = args[key];
  return typeof value === "boolean" ? value : false;
}

function optStringArray(args: Record<string, unknown>, key: string): string[] {
  const value = args[key];
  if (Array.isArray(value)) {
    return value.filter((x): x is string => typeof x === "string");
  }
  return [];
}

function optUint(args: Record<string, unknown>, key: string): number | undefined {
  const value = args[key];
  if (typeof value === "number" && Number.isInteger(value) && value >= 0) {
    return value;
  }
  return undefined;
}

export function gitPlugin(host: MockProjectHost): MockPluginFactory {
  let isRepo = true;
  let currentBranch = "main";
  const branches = new Set<string>(["main", "feature/bounds"]);
  const stagedPaths = new Set<string>();
  /** Full text of every tracked file in each commit, by commit hash. */
  const snapshots = new Map<string, Map<string, string>>();
  const labelled = new Set<string>();
  /** `undefined` follows the dev flag; `null` is "no identity yet". */
  let identity: { name: string; email: string } | null | undefined;
  const DEFAULT_IDENTITY = { name: "Rolf Landauer", email: "landauer@ibm.com" };

  function currentIdentity(): { name: string; email: string } | null {
    if (identity !== undefined) return identity;
    return identityFlagIsNone() ? null : DEFAULT_IDENTITY;
  }

  function requireIdentity(): { name: string; email: string } {
    const id = currentIdentity();
    if (id === null) {
      gitFail(
        "missing-identity",
        "git has no author identity; set your name and email, then commit again",
      );
    }
    return id;
  }

  /** The history the demo project starts with, taken from its committed files. */
  function ensureSnapshots(fs: MockProjectFs): void {
    if (snapshots.size > 0) return;
    const head = commits[0];
    const first = commits[1];
    if (head !== undefined) snapshots.set(head.hash, new Map(fs.baseline));
    if (first !== undefined) {
      const older = new Map(fs.baseline);
      const dropped = [...older.keys()].find((p) => p !== "manang.toml" && p !== fs.main);
      if (dropped !== undefined) older.delete(dropped);
      snapshots.set(first.hash, older);
    }
  }

  function snapshotOf(hash: string): Map<string, string> {
    return snapshots.get(hash) ?? new Map<string, string>();
  }

  function parentSnapshot(index: number): Map<string, string> {
    const parent = commits[index + 1];
    return parent === undefined ? new Map<string, string>() : snapshotOf(parent.hash);
  }

  /** Paths whose text differs between a commit and its parent. */
  function changedPaths(index: number): string[] {
    const commit = commits[index];
    if (commit === undefined) return [];
    const now = snapshotOf(commit.hash);
    const before = parentSnapshot(index);
    const paths = new Set([...now.keys(), ...before.keys()]);
    return [...paths].filter((p) => now.get(p) !== before.get(p)).sort();
  }

  function resolveRev(rev: string): number {
    if (rev === "" || rev.startsWith("-") || /[^A-Za-z0-9._/~^@{}-]/.test(rev) || rev.includes("..")) {
      gitFail("bad-arguments", `invalid revision \`${rev}\``);
    }
    const index = rev === "HEAD" ? 0 : commits.findIndex((c) => c.hash.startsWith(rev));
    if (index < 0) gitFail("rev-not-found", `revision \`${rev}\` was not found`);
    return index;
  }

  function recordCommit(fs: MockProjectFs, subject: string, label: boolean): GitCommitInfo {
    const id = requireIdentity();
    const hash = (Math.random().toString(16).slice(2) + "0".repeat(40)).slice(0, 40);
    const commit: GitCommitInfo = {
      hash,
      short: hash.slice(0, 7),
      author_name: id.name,
      author_email: id.email,
      time: Math.floor(Date.now() / 1000),
      subject,
      parents: commits.length > 0 ? [commits[0]!.hash] : [],
    };
    commits.unshift(commit);
    snapshots.set(hash, new Map(fs.baseline));
    if (label) labelled.add(hash);
    return commit;
  }

  /** Make every non-ignored change part of the committed state. */
  function stageEverything(fs: MockProjectFs): number {
    let changed = 0;
    for (const [path, value] of fs.files) {
      if (typeof value !== "string") continue;
      if (fs.baseline.get(path) !== value) {
        fs.baseline.set(path, value);
        changed += 1;
      }
    }
    for (const path of [...fs.baseline.keys()]) {
      if (!fs.files.has(path)) {
        fs.baseline.delete(path);
        changed += 1;
      }
    }
    return changed;
  }

  function hasChanges(fs: MockProjectFs): boolean {
    for (const [path, value] of fs.files) {
      if (typeof value === "string" && fs.baseline.get(path) !== value) return true;
    }
    for (const path of fs.baseline.keys()) if (!fs.files.has(path)) return true;
    return false;
  }

  function saveVersion(fs: MockProjectFs, name: string | undefined): GitSavedVersion {
    requireIdentity();
    if (!hasChanges(fs)) {
      gitFail("nothing-to-save", "there is nothing to save: no files have changed since the last version");
    }
    const subject = name !== undefined && name.trim() !== "" ? name.trim() : defaultVersionName(new Date());
    if (subject.includes("\n")) {
      gitFail("bad-arguments", "a version name is one line of at most 200 characters");
    }
    const filesChanged = stageEverything(fs);
    stagedPaths.clear();
    const commit = recordCommit(fs, subject, true);
    return { sha: commit.hash, short: commit.short, subject, files_changed: filesChanged };
  }

  const commits: GitCommitInfo[] = [
    {
      hash: "d3b8902abc1234567890abcdef1234567890abcdef",
      short: "d3b8902",
      author_name: "Rolf Landauer",
      author_email: "landauer@ibm.com",
      time: Math.floor(Date.now() / 1000) - 3600,
      subject: "feat(bounds): update irreversible dissipation curve",
      parents: ["fa1e932"],
    },
    {
      hash: "fa1e932abc1234567890abcdef1234567890abcdef",
      short: "fa1e932",
      author_name: "Rolf Landauer",
      author_email: "landauer@ibm.com",
      time: Math.floor(Date.now() / 1000) - 86400,
      subject: "chore: initial thermodynamic model and notes",
      parents: [],
    },
  ];

  function getFs(rootArg?: string): MockProjectFs {
    const fs = resolveFs(rootArg);
    ensureSnapshots(fs);
    return fs;
  }

  function resolveFs(rootArg?: string): MockProjectFs {
    if (rootArg) {
      const current = host.current;
      if (current === null || current.root !== rootArg) {
        try {
          return host.open(rootArg);
        } catch {
          // ignore
        }
      }
    }
    const fs = host.current;
    if (fs === null) {
      throw { message: JSON.stringify({ code: "no-root", message: "no project root is set" }) };
    }
    return fs;
  }

  function computeStatusFiles(fs: MockProjectFs): GitStatusFile[] {
    const statusFiles: GitStatusFile[] = [];

    // Compare fs.files against fs.baseline
    for (const [path, content] of fs.files.entries()) {
      const baselineContent = fs.baseline.get(path);
      const isStaged = stagedPaths.has(path);

      if (baselineContent === undefined) {
        // Not in baseline: untracked or staged new file
        if (isStaged) {
          statusFiles.push({
            path,
            staged: "added",
            unstaged: null,
            untracked: false,
            conflicted: false,
          });
        } else {
          statusFiles.push({
            path,
            staged: null,
            unstaged: null,
            untracked: true,
            conflicted: false,
          });
        }
      } else {
        const text = typeof content === "string" ? content : "";
        if (text !== baselineContent) {
          statusFiles.push({
            path,
            staged: isStaged ? "modified" : null,
            unstaged: isStaged ? null : "modified",
            untracked: false,
            conflicted: false,
          });
        } else if (isStaged) {
          statusFiles.push({
            path,
            staged: "modified",
            unstaged: null,
            untracked: false,
            conflicted: false,
          });
        }
      }
    }

    // Check for deleted files
    for (const [path] of fs.baseline.entries()) {
      if (!fs.files.has(path)) {
        const isStaged = stagedPaths.has(path);
        statusFiles.push({
          path,
          staged: isStaged ? "deleted" : null,
          unstaged: isStaged ? null : "deleted",
          untracked: false,
          conflicted: false,
        });
      }
    }

    statusFiles.sort((a, b) => a.path.localeCompare(b.path));
    return statusFiles;
  }

  return (emit): MockPlugin => ({
    name: "git",
    commands: COMMANDS,
    handlers: {
      "git.open": (args) => {
        const root = str(args, "root");
        // Opening a project again must not reset its files to the seed.
        if (host.current?.root !== root) {
          try {
            host.open(root);
          } catch {
            // ignore
          }
        }
        return { root, is_repo: isRepo };
      },

      "git.init": (args) => {
        isRepo = true;
        const fs = getFs(optStr(args, "root"));
        const files = computeStatusFiles(fs);
        emit("git.changed", { root: fs.root });
        return {
          branch: currentBranch,
          detached: false,
          unborn: false,
          head: commits[0]?.hash ?? null,
          upstream: null,
          files,
          truncated: false,
        };
      },

      "git.status": (args) => {
        if (!isRepo) {
          throw {
            message: JSON.stringify({
              code: "not-a-repo",
              message: "not inside a git repository; run git.init to create one",
            }),
          };
        }
        const fs = getFs(optStr(args, "root"));
        const files = computeStatusFiles(fs);
        const report: GitStatusReport = {
          branch: currentBranch,
          detached: false,
          unborn: false,
          head: commits[0]?.hash ?? null,
          upstream: { name: `origin/${currentBranch}`, ahead: 0, behind: 0 },
          files,
          truncated: false,
        };
        return report;
      },

      "git.diff": (args) => {
        if (!isRepo) {
          throw { message: JSON.stringify({ code: "not-a-repo", message: "not a git repo" }) };
        }
        const fs = getFs(optStr(args, "root"));
        const staged = optBool(args, "staged");
        const singlePath = optStr(args, "path");

        const statusFiles = computeStatusFiles(fs);
        const targetFiles = singlePath
          ? statusFiles.filter((f) => f.path === singlePath)
          : statusFiles.filter((f) => (staged ? f.staged !== null : f.unstaged !== null || f.untracked));

        const fileDiffs: GitFileDiff[] = targetFiles.map((f) => {
          const newRaw = fs.files.get(f.path);
          const diff = buildFileDiff(
            f.path,
            fs.baseline.get(f.path) ?? "",
            typeof newRaw === "string" ? newRaw : "",
          );
          return {
            ...diff,
            status: f.untracked ? "added" : f.unstaged === "deleted" ? "deleted" : "modified",
          };
        });

        const response: GitDiffResponse = {
          staged,
          files: fileDiffs,
        };
        return response;
      },

      "git.diff-text": (args) => {
        const path = str(args, "path");
        const fs = getFs(optStr(args, "root"));
        const oldContent = fs.baseline.get(path) ?? "";
        const newRaw = fs.files.get(path);
        const newContent = typeof newRaw === "string" ? newRaw : "";
        return { text: `--- a/${path}\n+++ b/${path}\n@@ -1 +1 @@\n-${oldContent}\n+${newContent}` };
      },

      "git.log": (args) => {
        const root = optStr(args, "root");
        if (root) getFs(root);
        const offset = optUint(args, "offset") ?? 0;
        const limit = optUint(args, "limit") ?? 50;
        const slice = commits.slice(offset, offset + limit);
        const response: GitLogResponse = {
          commits: slice,
          has_more: offset + limit < commits.length,
        };
        return response;
      },

      "git.blame": (args) => {
        const root = optStr(args, "root");
        if (root) getFs(root);
        const path = str(args, "path");
        return {
          path,
          total_lines: 1,
          ranges: [
            {
              start: 1,
              lines: 1,
              commit: commits[0]?.hash ?? "d3b8902",
              source_path: path,
            },
          ],
        };
      },

      "git.branches": (args) => {
        const root = optStr(args, "root");
        if (root) getFs(root);
        const list: GitBranchInfo[] = [...branches].map((name) => ({
          name,
          remote: false,
          current: name === currentBranch,
          hash: commits[0]?.hash ?? "d3b8902",
          upstream: `origin/${name}`,
        }));
        return list;
      },

      "git.checkout-branch": (args) => {
        const name = str(args, "name");
        const create = optBool(args, "create");
        if (create) {
          branches.add(name);
        } else if (!branches.has(name)) {
          throw {
            message: JSON.stringify({
              code: "bad-branch-name",
              message: `branch \`${name}\` not found`,
            }),
          };
        }
        currentBranch = name;
        const fs = getFs(optStr(args, "root"));
        emit("git.changed", { root: fs.root });
        return { branch: name, created: create };
      },

      "git.show": (args) => {
        const rev = str(args, "rev");
        const path = str(args, "path");
        const fs = getFs(optStr(args, "root"));
        const text = fs.baseline.get(path) ?? (typeof fs.files.get(path) === "string" ? fs.files.get(path) : "");
        return { rev, path, text: text as string };
      },

      "git.stage": (args) => {
        const paths = optStringArray(args, "paths");
        const fs = getFs(optStr(args, "root"));
        if (paths.includes(".")) {
          for (const p of fs.files.keys()) stagedPaths.add(p);
        } else {
          for (const p of paths) stagedPaths.add(p);
        }
        emit("git.changed", { root: fs.root });
        return { staged: paths.length };
      },

      "git.unstage": (args) => {
        const paths = optStringArray(args, "paths");
        const fs = getFs(optStr(args, "root"));
        if (paths.includes(".")) {
          stagedPaths.clear();
        } else {
          for (const p of paths) stagedPaths.delete(p);
        }
        emit("git.changed", { root: fs.root });
        return { unstaged: paths.length };
      },

      "git.discard": (args) => {
        const confirm = optBool(args, "confirm");
        if (!confirm) {
          throw {
            message: JSON.stringify({
              code: "confirm-required",
              message: "discarding changes is destructive; pass `confirm: true` to proceed",
            }),
          };
        }
        const paths = optStringArray(args, "paths");
        const fs = getFs(optStr(args, "root"));
        const targets = paths.includes(".") ? [...fs.files.keys()] : paths;

        for (const p of targets) {
          const base = fs.baseline.get(p);
          if (base !== undefined) {
            fs.files.set(p, base);
          } else {
            // Untracked file discarded
            fs.files.delete(p);
          }
          stagedPaths.delete(p);
        }

        emit("git.changed", { root: fs.root });
        emit("project.changed", {
          root: fs.root,
          changes: targets.map((p) => ({ path: p, kind: "modified" })),
        });
        return { discarded: targets, trash: ".git/manang-trash" };
      },

      "git.commit": (args) => {
        const message = str(args, "message").trim();
        const amend = optBool(args, "amend");

        if (message === "") {
          throw {
            message: JSON.stringify({
              code: "empty-message",
              message: "the commit message is empty",
            }),
          };
        }
        requireIdentity();
        const fs = getFs(optStr(args, "root"));
        if (optBool(args, "all")) {
          for (const p of fs.files.keys()) stagedPaths.add(p);
          for (const p of fs.baseline.keys()) stagedPaths.add(p);
        }

        if (stagedPaths.size === 0 && !amend) {
          throw {
            message: JSON.stringify({
              code: "nothing-to-commit",
              message: "there is nothing staged to commit",
            }),
          };
        }

        // Update baseline with staged content
        for (const p of stagedPaths) {
          const content = fs.files.get(p);
          if (typeof content === "string") {
            fs.baseline.set(p, content);
          } else if (content === undefined) {
            fs.baseline.delete(p);
          }
        }
        stagedPaths.clear();

        const subject = message.split("\n")[0] ?? message;
        const newCommit = recordCommit(fs, subject, false);
        if (amend && commits.length > 1) {
          // Replace the previous head with the amended commit.
          commits.splice(1, 1);
        }

        emit("git.changed", { root: fs.root });
        emit("project.changed", { root: fs.root, changes: [] });

        return {
          hash: newCommit.hash,
          short: newCommit.short,
          subject,
          amended: amend,
        };
      },

      "git.identity-get": (args) => {
        getFs(optStr(args, "root"));
        const id = currentIdentity();
        const result: GitIdentity =
          id === null
            ? { name: null, email: null, source: "none" }
            : { name: id.name, email: id.email, source: identity === undefined ? "global" : "repo" };
        return result;
      },

      "git.identity-set": (args) => {
        const name = str(args, "name").trim();
        const email = str(args, "email").trim();
        if (name === "" || [...name].some((c) => c.charCodeAt(0) < 32)) {
          gitFail("bad-arguments", "the name must not be empty or contain line breaks");
        }
        if (!/^[^\s@<>]+@[^\s@<>]+\.[^\s@<>.]+$/.test(email)) {
          gitFail("bad-arguments", "that does not look like an email address");
        }
        identity = { name, email };
        return { ok: true };
      },

      "git.save-version": (args) => {
        const fs = getFs(optStr(args, "root"));
        const saved = saveVersion(fs, optStr(args, "name"));
        emit("git.changed", { root: fs.root });
        emit("project.changed", { root: fs.root, changes: [] });
        return saved;
      },

      "git.versions": (args) => {
        getFs(optStr(args, "root"));
        const path = optStr(args, "path") ?? "";
        const limit = optUint(args, "limit") ?? 50;
        const cursor = optStr(args, "cursor");
        const offset = cursor === undefined ? 0 : Number(cursor);
        if (!Number.isInteger(offset) || offset < 0) gitFail("bad-arguments", "invalid cursor");
        const indices = commits
          .map((_, i) => i)
          .filter((i) => {
            if (path === "") return true;
            const touched = changedPaths(i);
            return touched.some((p) => p === path || p.startsWith(`${path}/`));
          });
        const page = indices.slice(offset, offset + limit);
        const versions: GitVersionInfo[] = page.map((i) => {
          const c = commits[i]!;
          return {
            hash: c.hash,
            short: c.short,
            subject: c.subject,
            author_name: c.author_name,
            time: c.time,
            files_changed: changedPaths(i).length,
            labelled: labelled.has(c.hash),
          };
        });
        const result: GitVersionsPage = {
          versions,
          next_cursor: offset + limit < indices.length ? String(offset + limit) : null,
        };
        return result;
      },

      "git.version-files": (args) => {
        getFs(optStr(args, "root"));
        const rev = str(args, "rev");
        const index = resolveRev(rev);
        const now = snapshotOf(commits[index]!.hash);
        const before = parentSnapshot(index);
        const files = changedPaths(index).map((p) => ({
          ...buildFileDiff(p, before.get(p), now.get(p)),
          hunks: [],
        }));
        return { rev, files };
      },

      "git.version-diff": (args) => {
        getFs(optStr(args, "root"));
        const index = resolveRev(str(args, "rev"));
        const path = str(args, "path");
        if (!changedPaths(index).includes(path)) {
          gitFail("path-not-in-rev", `\`${path}\` does not exist at that revision`);
        }
        return buildFileDiff(
          path,
          parentSnapshot(index).get(path),
          snapshotOf(commits[index]!.hash).get(path),
        );
      },

      "git.restore": (args) => {
        const fs = getFs(optStr(args, "root"));
        const index = resolveRev(str(args, "rev"));
        const whole = optBool(args, "whole");
        const paths = optStringArray(args, "paths");
        const saveFirst = optBool(args, "save_first");
        if (!whole && paths.length === 0) {
          gitFail("bad-arguments", "name the `paths` to restore, or pass `whole: true`");
        }
        const commit = commits[index]!;
        const snap = snapshotOf(commit.hash);
        const inScope = (p: string, scope: string): boolean =>
          p === scope || p.startsWith(`${scope}/`);
        let targets = [...snap.keys()];
        if (!whole) {
          for (const scope of paths) {
            if (scope.split("/").includes("..") || scope.startsWith("/")) {
              gitFail("bad-path", `\`${scope}\` is outside the project`);
            }
            if (!targets.some((p) => inScope(p, scope))) {
              gitFail("path-not-in-rev", `\`${scope}\` does not exist at that revision`);
            }
          }
          targets = targets.filter((p) => paths.some((scope) => inScope(p, scope)));
        }
        targets.sort();
        const replace = targets.filter((p) => fs.files.get(p) !== snap.get(p));
        const changed = new Set(computeStatusFiles(fs).map((f) => f.path));
        const dirty = replace.filter((p) => fs.files.has(p) && changed.has(p));
        if (dirty.length > 0 && !saveFirst) {
          gitFail(
            "dirty-tree",
            `${dirty.length} file(s) have changes that restoring would overwrite; save a version first (or pass \`save_first\`): ${dirty.join(", ")}`,
            { paths: dirty },
          );
        }
        let safety: string | null = null;
        if (dirty.length > 0) {
          try {
            safety = saveVersion(fs, `Before restoring ${commit.short}`).sha;
          } catch (error) {
            const code = (() => {
              try {
                return (JSON.parse((error as { message: string }).message) as { code?: string }).code;
              } catch {
                return undefined;
              }
            })();
            if (code !== "nothing-to-save") throw error;
          }
        }
        const created = new Set(replace.filter((p) => !fs.files.has(p)));
        for (const p of replace) fs.files.set(p, snap.get(p) ?? "");
        const untouched = whole
          ? [...fs.files.keys()].filter((p) => typeof fs.files.get(p) === "string" && !snap.has(p)).sort()
          : [];
        const result: GitRestored = {
          restored: replace,
          untouched_new: untouched,
          safety_version: safety,
          trash: dirty.length > 0 || replace.length > 0 ? ".git/manang-trash" : null,
        };
        emit("git.changed", { root: fs.root });
        emit("project.changed", {
          root: fs.root,
          changes: replace.map((path) => ({
            path,
            kind: created.has(path) ? "created" : "modified",
          })),
        });
        return result;
      },

      "git.sync": (args) => {
        const action = str(args, "action");
        const fs = getFs(optStr(args, "root"));
        emit("git.changed", { root: fs.root });
        return {
          action,
          remote: "origin",
          output: "Already up to date.",
        };
      },
    },
  });
}
