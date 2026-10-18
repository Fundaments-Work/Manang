/**
 * In-memory `recovery` plugin: `crates/manang-recovery` for the browser.
 *
 * Snapshots live in `localStorage` (so a page reload, the browser's "crash",
 * keeps them) with an in-memory fallback when storage is unavailable. Same
 * commands, arguments, size cap and result shapes as the Rust store. Keep in
 * step with `crates/manang-recovery/manifest.toml`.
 */

import type {
  RecoveryDisk,
  RecoveryEntry,
  RecoverySnapshot,
  RecoveryWrite,
} from "./api";
import type { MockPlugin, MockPluginFactory } from "./mock";
import { mockHash } from "./mockDiff";
import type { MockProjectHost } from "./mockProject";

/** The same cap as `MAX_SNAPSHOT_BYTES` in Rust. */
export const MOCK_MAX_SNAPSHOT_BYTES = 5 * 1024 * 1024;
export const RECOVERY_STORAGE_KEY = "manang.recovery.v1";

const COMMANDS: readonly (readonly [string, string])[] = [
  ["recovery.write", "Store a recovery snapshot of an unsaved document"],
  ["recovery.list", "List a project's recovery snapshots"],
  ["recovery.read", "Read one recovery snapshot"],
  ["recovery.discard", "Remove recovery snapshots"],
];

interface Stored {
  version: 1;
  snapshots: Record<string, RecoverySnapshot>;
}

function cleanPath(raw: unknown): string {
  if (typeof raw !== "string" || raw === "") {
    throw { message: "bad arguments: `path` must be a string" };
  }
  if (raw.startsWith("/") || raw.split("/").includes("..") || raw.includes("\\")) {
    throw { message: `invalid path: \`${raw}\` must stay inside the project` };
  }
  return raw.replace(/^\.\//, "");
}

function projectOf(args: Record<string, unknown>): string {
  const project = args["project"];
  if (typeof project !== "string" || project === "") {
    throw { message: "bad arguments: `project` must be a string" };
  }
  return project.replace(/\/+$/, "");
}

const key = (project: string, path: string): string => `${project}\n${path}`;

export function recoveryPlugin(host: MockProjectHost): MockPluginFactory {
  return (): MockPlugin => {
    let memory: Stored = { version: 1, snapshots: {} };

    const load = (): Stored => {
      try {
        const raw = localStorage.getItem(RECOVERY_STORAGE_KEY);
        if (raw === null) return memory;
        const parsed = JSON.parse(raw) as Stored;
        if (parsed.version === 1 && typeof parsed.snapshots === "object")
          memory = parsed;
      } catch {
        // Storage unavailable or the entry is damaged: keep what we hold.
      }
      return memory;
    };
    const save = (stored: Stored): void => {
      memory = stored;
      try {
        localStorage.setItem(RECOVERY_STORAGE_KEY, JSON.stringify(stored));
      } catch {
        // In-memory only.
      }
    };

    const diskState = (snapshot: RecoverySnapshot): RecoveryDisk => {
      const fs = host.current;
      if (fs === null || fs.root !== snapshot.project) return "unknown";
      const disk = fs.files.get(snapshot.path);
      if (disk === undefined) return "missing";
      if (typeof disk !== "string") return "unknown";
      if (snapshot.saved_hash === null) {
        return snapshot.kind === "figure" ? "unknown" : "changed";
      }
      return mockHash(disk) === snapshot.saved_hash ? "match" : "changed";
    };

    return {
      name: "recovery",
      commands: COMMANDS,
      handlers: {
        "recovery.write": (args): RecoveryWrite => {
          const project = projectOf(args);
          const path = cleanPath(args["path"]);
          const content = args["content"];
          const kind = args["kind"];
          if (typeof content !== "string") {
            throw { message: "bad arguments: `content` must be a string" };
          }
          if (kind !== "text" && kind !== "figure") {
            throw { message: "bad arguments: `kind` must be text or figure" };
          }
          if (new TextEncoder().encode(content).length > MOCK_MAX_SNAPSHOT_BYTES) {
            return {
              stored: false,
              reason: "too-large",
              limit: MOCK_MAX_SNAPSHOT_BYTES,
            };
          }
          const stored = load();
          const savedHash = args["saved_hash"];
          stored.snapshots[key(project, path)] = {
            version: 1,
            kind,
            project,
            path,
            saved_hash: typeof savedHash === "string" ? savedHash : null,
            saved_at: Date.now(),
            text: content,
            ...(args["view"] !== undefined ? { view: args["view"] } : {}),
          };
          save(stored);
          return { stored: true, limit: MOCK_MAX_SNAPSHOT_BYTES };
        },
        "recovery.list": (args) => {
          const project = projectOf(args);
          const entries: RecoveryEntry[] = Object.values(load().snapshots)
            .filter((s) => s.project === project)
            .map((s) => ({
              kind: s.kind,
              path: s.path,
              saved_at: s.saved_at,
              bytes: s.text.length,
              disk: diskState(s),
            }))
            .sort((a, b) => b.saved_at - a.saved_at || a.path.localeCompare(b.path));
          return { entries, set_aside: 0 };
        },
        "recovery.read": (args) => {
          const project = projectOf(args);
          return load().snapshots[key(project, cleanPath(args["path"]))] ?? null;
        },
        "recovery.discard": (args) => {
          const project = projectOf(args);
          const stored = load();
          let removed = 0;
          if (args["path"] === undefined || args["path"] === null) {
            for (const k of Object.keys(stored.snapshots)) {
              if (stored.snapshots[k]?.project === project) {
                delete stored.snapshots[k];
                removed += 1;
              }
            }
          } else {
            const k = key(project, cleanPath(args["path"]));
            if (stored.snapshots[k] !== undefined) {
              delete stored.snapshots[k];
              removed = 1;
            }
          }
          save(stored);
          return { removed };
        },
      },
    };
  };
}
