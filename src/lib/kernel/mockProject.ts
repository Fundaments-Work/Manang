/**
 * In-memory `project` plugin.
 *
 * Reproduces `crates/manang-project`'s command surface (names, arguments,
 * result shapes, failure messages) over a fake folder held in memory, so the
 * browser dev mode and the headless tests exercise every Files, Search and
 * References path. Mirrors `manifest.toml`; keep the two in step.
 */

import type { MockPlugin, MockPluginFactory } from "./mock";
import type {
  ChangeState,
  Engine,
  GitChange,
  GitStatus,
  ProjectConfig,
  ProjectSummary,
  RecentProject,
  TreeEntry,
  TreePage,
} from "./types";

type Bytes = string | Uint8Array;

export interface SeedProject {
  readonly root: string;
  readonly name: string;
  readonly engine: Engine;
  readonly main: string;
  readonly files: Readonly<Record<string, Bytes>>;
  /** Content `git` already knows; anything else is untracked or modified. */
  readonly committed: readonly string[];
  readonly openedAt: number;
}

export interface MockProjectFs {
  readonly root: string;
  readonly name: string;
  readonly engine: Engine;
  readonly main: string;
  readonly files: Map<string, Bytes>;
  readonly baseline: Map<string, string>;
}

/** Shared by the project and typst mocks: which project is open right now. */
export class MockProjectHost {
  readonly #seeds = new Map<string, SeedProject>();
  readonly #recent = new Map<string, { name: string; openedAt: number }>();
  #current: MockProjectFs | null = null;

  constructor(seeds: readonly SeedProject[], extraRecent: readonly RecentSeed[] = []) {
    for (const seed of seeds) {
      this.#seeds.set(seed.root, seed);
      this.#recent.set(seed.root, { name: seed.name, openedAt: seed.openedAt });
    }
    for (const entry of extraRecent) {
      this.#recent.set(entry.path, { name: entry.name, openedAt: entry.openedAt });
    }
  }

  get current(): MockProjectFs | null {
    return this.#current;
  }

  require(): MockProjectFs {
    if (this.#current === null) throw { message: "no project is open" };
    return this.#current;
  }

  open(path: string): MockProjectFs {
    const seed = this.#seeds.get(path);
    if (seed === undefined) {
      throw { message: `${path}: not a Manang project (no manang.toml found)` };
    }
    this.#current = materialise(seed);
    this.#touch(seed.root, seed.name);
    return this.#current;
  }

  create(path: string, name: string, engine: Engine): MockProjectFs {
    if (this.#seeds.has(path)) throw { message: `${path} is already a Manang project` };
    const main = MAIN_FILE[engine];
    const seed: SeedProject = {
      root: path,
      name,
      engine,
      main,
      files: { [main]: starter(engine, name) },
      committed: [],
      openedAt: nowSeconds(),
    };
    this.#seeds.set(path, seed);
    this.#current = materialise(seed);
    this.#touch(path, name);
    return this.#current;
  }

  close(): void {
    this.#current = null;
  }

  recent(): RecentProject[] {
    return [...this.#recent.entries()]
      .map(([path, value]) => ({
        path,
        name: value.name,
        opened_at: value.openedAt,
        exists: this.#seeds.has(path),
      }))
      .sort((a, b) => b.opened_at - a.opened_at);
  }

  #touch(path: string, name: string): void {
    this.#recent.set(path, { name, openedAt: nowSeconds() });
  }
}

export interface RecentSeed {
  readonly path: string;
  readonly name: string;
  readonly openedAt: number;
}

const MAIN_FILE: Record<Engine, string> = {
  typst: "main.typ",
  latex: "main.tex",
  markdown: "index.md",
};

function nowSeconds(): number {
  return Math.floor(Date.now() / 1000);
}

function starter(engine: Engine, name: string): string {
  switch (engine) {
    case "typst":
      return `= ${name}\n\nStart writing here.\n`;
    case "latex":
      return `\\documentclass{article}\n\\begin{document}\n\\section{${name}}\nStart writing here.\n\\end{document}\n`;
    case "markdown":
      return `# ${name}\n\nStart writing here.\n`;
  }
}

function materialise(seed: SeedProject): MockProjectFs {
  const files = new Map<string, Bytes>(Object.entries(seed.files));
  const baseline = new Map<string, string>();
  for (const path of seed.committed) {
    const value = seed.files[path];
    if (typeof value === "string") baseline.set(path, value);
  }
  // `manang.toml` is part of every project and is committed with it.
  const config = configText(seed);
  files.set("manang.toml", config);
  baseline.set("manang.toml", config);
  return {
    root: seed.root,
    name: seed.name,
    engine: seed.engine,
    main: seed.main,
    files,
    baseline,
  };
}

function configText(seed: SeedProject): string {
  return `[project]\nengine = "${seed.engine}"\nname = "${seed.name}"\nmain = "${seed.main}"\n`;
}

/* -------------------------------------------------------------------------- */

function confine(raw: unknown, key = "path"): string {
  if (typeof raw !== "string") throw { message: `bad arguments: \`${key}\` must be a string` };
  const path = raw.replace(/\\/g, "/").replace(/^\.\//, "").replace(/\/+$/, "");
  if (path.startsWith("/") || path.split("/").includes("..")) {
    throw { message: `${raw}: path escapes the project` };
  }
  return path;
}

function dirOf(path: string): string {
  const at = path.lastIndexOf("/");
  return at === -1 ? "" : path.slice(0, at);
}

function nameOf(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

function isDir(fs: MockProjectFs, path: string): boolean {
  const prefix = `${path}/`;
  for (const key of fs.files.keys()) if (key.startsWith(prefix)) return true;
  return false;
}

export function listTree(
  fs: MockProjectFs,
  query: { path?: string; recursive?: boolean; offset?: number; limit?: number },
): TreePage {
  const base = confine(query.path ?? "");
  if (base !== "" && !isDir(fs, base)) {
    throw { message: `${base}: No such file or directory` };
  }
  const prefix = base === "" ? "" : `${base}/`;
  const found = new Map<string, TreeEntry>();
  for (const path of fs.files.keys()) {
    if (!path.startsWith(prefix)) continue;
    const rest = path.slice(prefix.length);
    const parts = rest.split("/");
    if (query.recursive === true) {
      for (let i = 1; i < parts.length; i += 1) {
        const dir = prefix + parts.slice(0, i).join("/");
        found.set(dir, { path: dir, name: parts[i - 1] ?? dir, kind: "dir" });
      }
      found.set(path, { path, name: nameOf(path), kind: "file" });
    } else if (parts.length === 1) {
      found.set(path, { path, name: nameOf(path), kind: "file" });
    } else {
      const dir = prefix + (parts[0] ?? "");
      found.set(dir, { path: dir, name: parts[0] ?? dir, kind: "dir" });
    }
  }
  const all = [...found.values()].sort((a, b) => a.path.localeCompare(b.path));
  const offset = query.offset ?? 0;
  const limit = Math.min(query.limit ?? 1000, 5000);
  const entries = all.slice(offset, offset + limit);
  return {
    entries,
    total: all.length,
    offset,
    limit,
    has_more: offset + entries.length < all.length,
    truncated: false,
  };
}

export function gitStatus(fs: MockProjectFs): GitStatus {
  const changes: GitChange[] = [];
  const state = (value: ChangeState): ChangeState => value;
  for (const [path, value] of fs.files) {
    if (typeof value !== "string") continue;
    const before = fs.baseline.get(path);
    if (before === undefined) {
      changes.push({ path, staged: null, unstaged: state("untracked") });
    } else if (before !== value) {
      changes.push({ path, staged: null, unstaged: state("modified") });
    }
  }
  for (const path of fs.baseline.keys()) {
    if (!fs.files.has(path)) {
      changes.push({ path, staged: null, unstaged: state("deleted") });
    }
  }
  changes.sort((a, b) => a.path.localeCompare(b.path));
  return {
    is_repo: true,
    branch: "main",
    detached: false,
    head: "3f9c2ab71d04e58b6a0c9e2d41f7b835ac06d19e",
    changes,
    truncated: false,
  };
}

function configOf(fs: MockProjectFs): ProjectConfig {
  const bibs = [...fs.files.keys()].filter((path) => path.endsWith(".bib"));
  return {
    project: { engine: fs.engine, name: fs.name, main: fs.main },
    ...(bibs.length > 0 ? { bibliography: { paths: bibs } } : {}),
  };
}

function summary(fs: MockProjectFs): ProjectSummary {
  return {
    root: fs.root,
    name: fs.name,
    engine: fs.engine,
    main: fs.main,
    config: configOf(fs),
    warnings: [],
    tree: listTree(fs, {}),
  };
}

const COMMANDS: readonly (readonly [string, string])[] = [
  ["project.open", "Open project"],
  ["project.close", "Close project"],
  ["project.create", "New project"],
  ["project.config", "Read project configuration"],
  ["project.tree", "List project files"],
  ["project.read-file", "Read text file"],
  ["project.read-bytes", "Read binary file"],
  ["project.write-file", "Write text file"],
  ["project.rename", "Rename or move file"],
  ["project.delete", "Move file to trash"],
  ["project.recent", "Recent projects"],
  ["project.git-status", "Git status"],
];

export function projectPlugin(host: MockProjectHost): MockPluginFactory {
  return (emit): MockPlugin => {
    const changed = (
      fs: MockProjectFs,
      changes: readonly { path: string; kind: string; from?: string }[],
    ): void => emit("project.changed", { root: fs.root, changes });

    const textOf = (fs: MockProjectFs, path: string): string => {
      const value = fs.files.get(path);
      if (value === undefined) throw { message: `${path}: No such file or directory` };
      if (typeof value !== "string") throw { message: `${path}: not a text file` };
      return value;
    };

    return {
      name: "project",
      commands: COMMANDS,
      handlers: {
        "project.open": (args) => summary(host.open(confineRoot(args))),
        "project.create": (args) => {
          const engine = args["engine"];
          const name = args["name"];
          if (engine !== "typst" && engine !== "latex" && engine !== "markdown") {
            throw { message: "bad arguments: `engine` must be typst, latex or markdown" };
          }
          if (typeof name !== "string" || name.trim() === "") {
            throw { message: "bad arguments: `name` must be a non-empty string" };
          }
          return summary(host.create(confineRoot(args), name.trim(), engine));
        },
        "project.close": () => {
          host.close();
          return null;
        },
        "project.config": () => configOf(host.require()),
        "project.tree": (args) =>
          listTree(host.require(), {
            ...(typeof args["path"] === "string" ? { path: args["path"] } : {}),
            ...(typeof args["recursive"] === "boolean"
              ? { recursive: args["recursive"] }
              : {}),
            ...(typeof args["offset"] === "number" ? { offset: args["offset"] } : {}),
            ...(typeof args["limit"] === "number" ? { limit: args["limit"] } : {}),
          }),
        "project.read-file": (args) => textOf(host.require(), confine(args["path"])),
        "project.write-file": (args) => {
          const fs = host.require();
          const path = confine(args["path"]);
          const content = args["content"];
          if (typeof content !== "string") {
            throw { message: "bad arguments: `content` must be a string" };
          }
          const existed = fs.files.has(path);
          fs.files.set(path, content);
          changed(fs, [{ path, kind: existed ? "modified" : "created" }]);
          return { bytes: new TextEncoder().encode(content).length };
        },
        "project.rename": (args) => {
          const fs = host.require();
          const from = confine(args["from"], "from");
          const to = confine(args["to"], "to");
          if (fs.files.has(to) || isDir(fs, to)) {
            throw { message: `${to}: already exists` };
          }
          const moves: [string, string][] = [];
          if (fs.files.has(from)) {
            moves.push([from, to]);
          } else if (isDir(fs, from)) {
            for (const key of fs.files.keys()) {
              if (key.startsWith(`${from}/`)) moves.push([key, to + key.slice(from.length)]);
            }
          } else {
            throw { message: `${from}: No such file or directory` };
          }
          for (const [source, target] of moves) {
            const value = fs.files.get(source);
            if (value === undefined) continue;
            fs.files.delete(source);
            fs.files.set(target, value);
          }
          changed(fs, [{ path: to, kind: "renamed", from }]);
          return null;
        },
        "project.delete": (args) => {
          const fs = host.require();
          const path = confine(args["path"]);
          const doomed = fs.files.has(path)
            ? [path]
            : [...fs.files.keys()].filter((key) => key.startsWith(`${path}/`));
          if (doomed.length === 0) throw { message: `${path}: No such file or directory` };
          for (const key of doomed) fs.files.delete(key);
          changed(
            fs,
            doomed.map((key) => ({ path: key, kind: "removed" })),
          );
          return { trashed_to: `${fs.root}/.manang/trash/${nameOf(path)}` };
        },
        "project.recent": () => host.recent(),
        "project.git-status": () => gitStatus(host.require()),
      },
      binaryHandlers: {
        "project.read-bytes": (args) => {
          const fs = host.require();
          const path = confine(args["path"]);
          const value = fs.files.get(path);
          if (value === undefined) throw { message: `${path}: No such file or directory` };
          return typeof value === "string" ? new TextEncoder().encode(value) : value;
        },
      },
    };
  };
}

function confineRoot(args: Record<string, unknown>): string {
  const path = args["path"];
  if (typeof path !== "string" || path.trim() === "") {
    throw { message: "bad arguments: `path` must be a string" };
  }
  return path.trim().replace(/\/+$/, "");
}

export { dirOf };
