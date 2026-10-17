/**
 * In-memory `git` plugin.
 *
 * Reproduces `crates/manang-git` command surface (15 commands)
 * over `MockProjectHost` in browser and tests.
 */

import type { MockPlugin, MockPluginFactory } from "./mock";
import type { MockProjectFs, MockProjectHost } from "./mockProject";
import type {
  GitBranchInfo,
  GitCommitInfo,
  GitDiffHunk,
  GitDiffLine,
  GitDiffResponse,
  GitFileDiff,
  GitLogResponse,
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
  ["git.init", "Initialise repository"],
  ["git.log", "Commit log"],
  ["git.open", "Set the git project root"],
  ["git.show", "File content at revision"],
  ["git.stage", "Stage files"],
  ["git.status", "Git status"],
  ["git.sync", "Fetch, pull or push"],
  ["git.unstage", "Unstage files"],
];

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
        try {
          host.open(root);
        } catch {
          // ignore
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
          const oldContent = fs.baseline.get(f.path) ?? "";
          const newRaw = fs.files.get(f.path);
          const newContent = typeof newRaw === "string" ? newRaw : "";

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
            path: f.path,
            status: f.untracked ? "added" : f.unstaged === "deleted" ? "deleted" : "modified",
            binary: false,
            too_large: false,
            additions,
            deletions,
            hunks: [hunk],
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

        if (stagedPaths.size === 0 && !amend) {
          throw {
            message: JSON.stringify({
              code: "nothing-to-commit",
              message: "there is nothing staged to commit",
            }),
          };
        }

        const fs = getFs(optStr(args, "root"));
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

        const hash = Math.random().toString(16).slice(2).padStart(40, "0");
        const short = hash.slice(0, 7);
        const subject = message.split("\n")[0] ?? message;

        const newCommit: GitCommitInfo = {
          hash,
          short,
          author_name: "Author Name",
          author_email: "author@example.com",
          time: Math.floor(Date.now() / 1000),
          subject,
          parents: commits.length > 0 ? [commits[0]!.hash] : [],
        };

        if (amend && commits.length > 0) {
          commits[0] = newCommit;
        } else {
          commits.unshift(newCommit);
        }

        emit("git.changed", { root: fs.root });
        emit("project.changed", { root: fs.root, changes: [] });

        return {
          hash,
          short,
          subject,
          amended: amend,
        };
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
