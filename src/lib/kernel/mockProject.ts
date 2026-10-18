/**
 * In-memory `project` plugin.
 *
 * Reproduces `crates/manang-project`'s command surface (names, arguments,
 * result shapes, failure messages) over a fake folder held in memory, so the
 * browser dev mode and the headless tests exercise every Files, Search and
 * References path. Mirrors `manifest.toml`; keep the two in step.
 */

import type { MockPlugin, MockPluginFactory } from "./mock";
import { mockKey, mockRender } from "./mockDiagram";
import { mockDiffText, mockHash } from "./mockDiff";
import {
  getMockTemplate,
  listMockTemplates,
  renderTemplateFiles,
} from "./mockTemplates";
import type {
  ChangeState,
  DiagramExport,
  DiagramLanguageId,
  Engine,
  FileStat,
  GitChange,
  GitStatus,
  ProjectConfig,
  ProjectEntry,
  ProjectListQuery,
  ProjectPatch,
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
  /** Files `git` already knows, at the content given in `files`. */
  readonly committed: readonly string[];
  /** Earlier committed content for files edited since, so they show as modified. */
  readonly committedEarlier?: Readonly<Record<string, string>>;
  readonly openedAt: number;
  /** Registry metadata, for the Home screen. */
  readonly pinned?: boolean;
  readonly archived?: boolean;
  readonly trashedAt?: string | null;
  readonly tags?: readonly string[];
  readonly created?: number;
  /** A ready-made thumbnail (SVG text), so a card has one before any compile. */
  readonly thumbnail?: string;
}

export interface MockProjectFs {
  readonly root: string;
  readonly name: string;
  readonly engine: Engine;
  readonly main: string;
  readonly files: Map<string, Bytes>;
  readonly baseline: Map<string, string>;
}

interface Meta {
  name: string;
  engine: Engine;
  created: number;
  openedAt: number;
  pinned: boolean;
  archived: boolean;
  trashedAt: string | null;
  tags: string[];
  thumbnail: string | null;
}

function unknownProject(path: string): { message: string } {
  return { message: "`" + path + "` does not exist" };
}

/** Shared by the project and typst mocks: which project is open right now. */
export class MockProjectHost {
  readonly #seeds = new Map<string, SeedProject>();
  readonly #meta = new Map<string, Meta>();
  #current: MockProjectFs | null = null;

  constructor(seeds: readonly SeedProject[], extraRecent: readonly RecentSeed[] = []) {
    for (const seed of seeds) {
      this.#seeds.set(seed.root, seed);
      this.#meta.set(seed.root, {
        name: seed.name,
        engine: seed.engine,
        created: seed.created ?? seed.openedAt,
        openedAt: seed.openedAt,
        pinned: seed.pinned ?? false,
        archived: seed.archived ?? false,
        trashedAt: seed.trashedAt ?? null,
        tags: [...(seed.tags ?? [])],
        thumbnail: seed.thumbnail ?? null,
      });
    }
    for (const entry of extraRecent) {
      this.#meta.set(entry.path, {
        name: entry.name,
        engine: entry.engine ?? "typst",
        created: entry.openedAt,
        openedAt: entry.openedAt,
        pinned: false,
        archived: false,
        trashedAt: null,
        tags: [],
        thumbnail: null,
      });
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
    this.#touch(seed.root, seed.name, seed.engine);
    return this.#current;
  }

  create(path: string, name: string, engine: Engine, template?: string): MockProjectFs {
    if (this.#seeds.has(path)) throw { message: `${path} is already a Manang project` };
    let main = MAIN_FILE[engine];
    let files: Record<string, Bytes> = { [main]: starter(engine, name) };
    if (template !== undefined) {
      const tmpl = getMockTemplate(template);
      if (tmpl !== undefined) {
        engine = tmpl.engine;
        main = tmpl.main;
        files = renderTemplateFiles(template, name) ?? files;
      }
    }
    const seed: SeedProject = {
      root: path,
      name,
      engine,
      main,
      files,
      committed: [],
      openedAt: nowSeconds(),
    };
    this.#seeds.set(path, seed);
    this.#current = materialise(seed);
    this.#touch(path, name, engine);
    return this.#current;
  }

  close(): void {
    this.#current = null;
  }

  recent(): RecentProject[] {
    return [...this.#meta.entries()]
      .map(([path, value]) => ({
        path,
        name: value.name,
        opened_at: value.openedAt,
        exists: this.#seeds.has(path),
      }))
      .sort((a, b) => b.opened_at - a.opened_at);
  }

  /** `project.list`: filtering out trashed unless `include_trashed` is true. */
  list(query?: ProjectListQuery): ProjectEntry[] {
    const entries = [...this.#meta.keys()].map((path) => this.entry(path));
    if (query?.include_trashed) return entries;
    return entries.filter((e) => !e.trashed_at);
  }

  entry(path: string): ProjectEntry {
    const meta = this.#meta.get(path);
    if (meta === undefined) throw unknownProject(path);
    return {
      path,
      name: meta.name,
      engine: meta.engine,
      created: meta.created,
      last_opened: meta.openedAt,
      pinned: meta.pinned,
      archived: meta.archived,
      trashed_at: meta.trashedAt,
      tags: [...meta.tags],
      exists: this.#seeds.has(path),
      has_thumbnail: meta.thumbnail !== null,
    };
  }

  trash(path: string): ProjectEntry {
    const meta = this.#meta.get(path);
    if (meta === undefined) throw unknownProject(path);
    if (this.#current?.root === path) this.#current = null;
    meta.trashedAt = new Date().toISOString();
    return this.entry(path);
  }

  restore(path: string): ProjectEntry {
    const meta = this.#meta.get(path);
    if (meta === undefined) throw unknownProject(path);
    meta.trashedAt = null;
    return this.entry(path);
  }

  purge(
    path: string,
    deleteFiles = false,
  ): { purged: boolean; deleted_files: boolean } {
    if (!this.#meta.has(path)) throw unknownProject(path);
    if (this.#current?.root === path) this.#current = null;
    this.#meta.delete(path);
    if (deleteFiles) {
      this.#seeds.delete(path);
    }
    return { purged: true, deleted_files: deleteFiles };
  }

  emptyTrash(olderThanDays?: number): { count: number } {
    const cutoff =
      olderThanDays === undefined ? null : Date.now() - olderThanDays * 86_400_000;
    let count = 0;
    for (const [path, meta] of [...this.#meta.entries()]) {
      if (meta.trashedAt === null) continue;
      if (cutoff !== null && Date.parse(meta.trashedAt) > cutoff) continue;
      if (this.#current?.root === path) this.#current = null;
      this.#meta.delete(path);
      count++;
    }
    return { count };
  }

  register(path: string): ProjectEntry {
    if (!this.#meta.has(path)) {
      const seed = this.#seeds.get(path);
      if (seed === undefined) {
        throw { message: `${path}: not a Manang project (no manang.toml found)` };
      }
      this.#touch(path, seed.name, seed.engine);
    }
    return this.entry(path);
  }

  update(path: string, patch: ProjectPatch): ProjectEntry {
    const meta = this.#meta.get(path);
    if (meta === undefined) throw unknownProject(path);
    if (patch.name !== undefined) {
      const name = patch.name.trim();
      if (name === "") {
        throw { message: "invalid project metadata: the name cannot be empty" };
      }
      meta.name = name;
    }
    if (patch.pinned !== undefined) meta.pinned = patch.pinned;
    if (patch.archived !== undefined) meta.archived = patch.archived;
    if (patch.tags !== undefined) {
      meta.tags = [...new Set(patch.tags.map((t) => t.trim()).filter((t) => t !== ""))];
    }
    return this.entry(path);
  }

  forget(path: string): boolean {
    return this.#meta.delete(path);
  }

  thumbnail(path: string): string | null {
    return this.#meta.get(path)?.thumbnail ?? null;
  }

  setThumbnail(path: string, svg: string): void {
    const meta = this.#meta.get(path);
    if (meta === undefined) throw unknownProject(path);
    meta.thumbnail = svg;
  }

  #touch(path: string, name: string, engine: Engine): void {
    const meta = this.#meta.get(path);
    if (meta === undefined) {
      const now = nowSeconds();
      this.#meta.set(path, {
        name,
        engine,
        created: now,
        openedAt: now,
        pinned: false,
        archived: false,
        trashedAt: null,
        tags: [],
        thumbnail: null,
      });
    } else {
      meta.openedAt = nowSeconds();
      meta.engine = engine;
    }
  }
}

export interface RecentSeed {
  readonly path: string;
  readonly name: string;
  readonly openedAt: number;
  readonly engine?: Engine;
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
  for (const [path, value] of Object.entries(seed.committedEarlier ?? {})) {
    baseline.set(path, value);
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
  if (typeof raw !== "string")
    throw { message: `bad arguments: \`${key}\` must be a string` };
  const path = raw.replace(/\\/g, "/").replace(/^\.\//, "").replace(/\/+$/, "");
  if (path.startsWith("/") || path.split("/").includes("..")) {
    throw { message: `${raw}: path escapes the project` };
  }
  return path;
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
    format_warnings: [],
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
  ["project.read-text", "Read text file with its fingerprint"],
  ["project.diff-text", "Line diff of a file on disk against given text"],
  ["project.backup", "Copy a file to the project trash before changing it"],
  ["project.rename", "Rename or move file"],
  ["project.delete", "Move file to trash"],
  ["project.recent", "Recent projects"],
  ["project.git-status", "Git status"],
  ["project.list", "List known projects"],
  ["project.register", "Add a project folder to the list"],
  ["project.update", "Rename, pin, archive or tag a project"],
  ["project.forget", "Remove a project from the list"],
  ["project.thumbnail-get", "Read a project thumbnail"],
  ["project.thumbnail-set", "Store a project thumbnail"],
  ["project.stat", "File size, date and kind"],
  ["project.export-diagram", "Write a diagram's SVG (and PDF for LaTeX) next to it"],
  ["project.cache-diagram", "Render an inline diagram to the project cache"],
  ["project.export-pdf", "Write a PDF next to an SVG for LaTeX"],
  ["project.trash", "Move project to trash"],
  ["project.restore", "Restore project from trash"],
  ["project.purge", "Purge project from trash"],
  ["project.empty-trash", "Empty trash"],
  ["project.templates", "List starter templates"],
];

/** Mirrors `crates/manang-project/src/export.rs`. */
const DIAGRAM_EXTENSIONS: Readonly<Record<string, DiagramLanguageId>> = {
  mmd: "mermaid",
  mermaid: "mermaid",
  pikchr: "pikchr",
  bob: "svgbob",
};

/** Stands in for `typst.svg-to-pdf`: a PDF header, clearly a mock. */
const MOCK_PDF = "%PDF-1.7\n% Manang browser mock: one page, no drawing.\n%%EOF\n";

function sibling(path: string, extension: string): string {
  const slash = path.lastIndexOf("/");
  const name = path.slice(slash + 1);
  const dot = name.lastIndexOf(".");
  const stem = dot > 0 ? name.slice(0, dot) : name;
  return `${path.slice(0, slash + 1)}${stem}.${extension}`;
}

function wantsPdf(fs: MockProjectFs): boolean {
  return (
    fs.engine === "latex" ||
    [...fs.files.keys()].some(
      (path) => path.toLowerCase().endsWith(".tex") && !path.startsWith(".manang/"),
    )
  );
}

function header(language: DiagramLanguageId, key: string): string {
  return `<!-- manang-diagram ${language} key=${key} -->\n`;
}

/** `{ ok: false }` with the mock renderer's message, or the SVG text. */
function renderPrint(
  language: DiagramLanguageId,
  source: string,
): { svg: string } | { error: string } {
  try {
    return { svg: mockRender(language, source, "print") };
  } catch (error) {
    return { error: (error as Error).message };
  }
}

export function projectPlugin(host: MockProjectHost): MockPluginFactory {
  return (emit): MockPlugin => {
    const changed = (
      fs: MockProjectFs,
      changes: readonly { path: string; kind: string; from?: string }[],
    ): void => emit("project.changed", { root: fs.root, changes });

    /** Modification times of files written since start; others are `opened`. */
    const modified = new Map<string, number>();
    const opened = Date.now();
    /** Write a file as the plugin would, with its change event. */
    const write = (fs: MockProjectFs, path: string, content: string): void => {
      const existed = fs.files.has(path);
      fs.files.set(path, content);
      modified.set(path, Date.now());
      changed(fs, [{ path, kind: existed ? "modified" : "created" }]);
    };

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
          const template =
            typeof args["template"] === "string" ? args["template"] : undefined;
          if (engine !== "typst" && engine !== "latex" && engine !== "markdown") {
            throw {
              message: "bad arguments: `engine` must be typst, latex or markdown",
            };
          }
          if (typeof name !== "string" || name.trim() === "") {
            throw { message: "bad arguments: `name` must be a non-empty string" };
          }
          return summary(host.create(confineRoot(args), name.trim(), engine, template));
        },
        "project.templates": () => listMockTemplates(),
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
          const expected = args["expected"];
          if (typeof expected === "string") {
            const current = fs.files.get(path);
            const fail = (what: string): never => {
              throw { message: `[conflict] \`${path}\` ${what}` };
            };
            if (expected === "absent") {
              if (current !== undefined) fail("already exists on disk");
            } else if (current === undefined) {
              fail("was deleted on disk since it was loaded");
            } else if (typeof current !== "string" || mockHash(current) !== expected) {
              fail("changed on disk since it was loaded");
            }
          }
          write(fs, path, content);
          return {
            bytes: new TextEncoder().encode(content).length,
            hash: mockHash(content),
          };
        },
        "project.read-text": (args) => {
          const fs = host.require();
          const path = confine(args["path"]);
          const content = textOf(fs, path);
          return {
            content,
            hash: mockHash(content),
            size: new TextEncoder().encode(content).length,
            modified: modified.get(path) ?? opened,
          };
        },
        "project.backup": (args) => {
          const fs = host.require();
          const path = confine(args["path"]);
          const value = fs.files.get(path);
          if (value === undefined) return { backup_to: null };
          const copy = `.manang/trash/${Date.now()}-${modified.size}/${path}`;
          fs.files.set(copy, value);
          return { backup_to: copy };
        },
        "project.diff-text": (args) => {
          const fs = host.require();
          const path = confine(args["path"]);
          const content = args["content"];
          if (typeof content !== "string") {
            throw { message: "bad arguments: `content` must be a string" };
          }
          const disk = fs.files.get(path);
          return mockDiffText(path, typeof disk === "string" ? disk : null, content);
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
              if (key.startsWith(`${from}/`))
                moves.push([key, to + key.slice(from.length)]);
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
          if (doomed.length === 0)
            throw { message: `${path}: No such file or directory` };
          for (const key of doomed) fs.files.delete(key);
          changed(
            fs,
            doomed.map((key) => ({ path: key, kind: "removed" })),
          );
          return { trashed_to: `${fs.root}/.manang/trash/${nameOf(path)}` };
        },
        "project.recent": () => host.recent(),
        "project.git-status": () => gitStatus(host.require()),
        "project.list": (args) => host.list(args as ProjectListQuery),
        "project.register": (args) => host.register(confineRoot(args)),
        "project.update": (args) => {
          const patch: {
            name?: string;
            pinned?: boolean;
            archived?: boolean;
            tags?: string[];
          } = {};
          if (typeof args["name"] === "string") patch.name = args["name"];
          if (typeof args["pinned"] === "boolean") patch.pinned = args["pinned"];
          if (typeof args["archived"] === "boolean") patch.archived = args["archived"];
          if (Array.isArray(args["tags"]))
            patch.tags = args["tags"].filter((t): t is string => typeof t === "string");
          return host.update(confineRoot(args), patch);
        },
        "project.forget": (args) => ({ removed: host.forget(confineRoot(args)) }),
        "project.trash": (args) => host.trash(confineRoot(args)),
        "project.restore": (args) => host.restore(confineRoot(args)),
        "project.purge": (args) =>
          host.purge(confineRoot(args), args["delete_files"] === true),
        "project.empty-trash": (args) =>
          host.emptyTrash(
            typeof args["older_than_days"] === "number"
              ? args["older_than_days"]
              : undefined,
          ),
        "project.thumbnail-set": (args) => {
          const svg = args["svg"];
          if (typeof svg !== "string" || !svg.trimStart().startsWith("<svg")) {
            throw {
              message: "invalid project metadata: the thumbnail is not an SVG document",
            };
          }
          host.setThumbnail(confineRoot(args), svg);
          return null;
        },
        "project.stat": (args): FileStat => {
          const fs = host.require();
          const path = confine(args["path"]);
          const value = fs.files.get(path);
          if (value === undefined) {
            if (path === "" || isDir(fs, path)) {
              return { size: 0, modified: modified.get(path) ?? opened, kind: "dir" };
            }
            throw { message: `\`${path}\` does not exist` };
          }
          const size =
            typeof value === "string"
              ? new TextEncoder().encode(value).length
              : value.byteLength;
          return { size, modified: modified.get(path) ?? opened, kind: "file" };
        },
        "project.export-diagram": (args): DiagramExport => {
          const fs = host.require();
          const path = confine(args["path"]);
          const extension = path.slice(path.lastIndexOf(".") + 1).toLowerCase();
          const language = DIAGRAM_EXTENSIONS[extension];
          if (language === undefined) {
            throw {
              message: `invalid path \`${path}\`: not a diagram file (.mmd, .mermaid, .pikchr, .bob)`,
            };
          }
          const source = textOf(fs, path);
          const svgPath = sibling(path, "svg");
          const top = header(language, mockKey(language, source, "print"));
          const existing = fs.files.get(svgPath);
          const written: string[] = [];
          let fresh = false;
          if (!(typeof existing === "string" && existing.startsWith(top))) {
            const rendered = renderPrint(language, source);
            if ("error" in rendered) {
              return {
                ok: false,
                svg: existing === undefined ? null : svgPath,
                pdf: null,
                written,
                error: rendered.error,
              };
            }
            write(fs, svgPath, top + rendered.svg);
            written.push(svgPath);
            fresh = true;
          }
          let pdf: string | null = null;
          if (wantsPdf(fs)) {
            pdf = sibling(path, "pdf");
            if (fresh || !fs.files.has(pdf)) {
              write(fs, pdf, MOCK_PDF);
              written.push(pdf);
            }
          }
          return { ok: true, svg: svgPath, pdf, written, error: null };
        },
        "project.cache-diagram": (args): DiagramExport => {
          const fs = host.require();
          const language = args["language"];
          const source = args["source"];
          if (
            (language !== "mermaid" &&
              language !== "pikchr" &&
              language !== "svgbob") ||
            typeof source !== "string"
          ) {
            throw { message: "bad arguments: `language` and `source` are required" };
          }
          const key = mockKey(language, source, "print");
          const svgPath = `.manang/cache/diagrams/${key}.svg`;
          if (fs.files.has(svgPath)) {
            return { ok: true, svg: svgPath, pdf: null, written: [], error: null };
          }
          const rendered = renderPrint(language, source);
          if ("error" in rendered) {
            return {
              ok: false,
              svg: null,
              pdf: null,
              written: [],
              error: rendered.error,
            };
          }
          fs.files.set(svgPath, header(language, key) + rendered.svg);
          return { ok: true, svg: svgPath, pdf: null, written: [svgPath], error: null };
        },
        "project.export-pdf": (args): DiagramExport => {
          const fs = host.require();
          const raw = args["path"];
          const prefix = `${fs.root}/`;
          const path = confine(
            typeof raw === "string" && raw.startsWith(prefix)
              ? raw.slice(prefix.length)
              : raw,
          );
          if (!path.toLowerCase().endsWith(".svg")) {
            throw { message: `invalid path \`${path}\`: not an SVG file` };
          }
          if (!(args["force"] === true || wantsPdf(fs))) {
            return { ok: true, svg: path, pdf: null, written: [], error: null };
          }
          const pdf = sibling(path, "pdf");
          if (fs.files.has(pdf)) {
            return { ok: true, svg: path, pdf, written: [], error: null };
          }
          write(fs, pdf, MOCK_PDF);
          return { ok: true, svg: path, pdf, written: [pdf], error: null };
        },
      },
      binaryHandlers: {
        "project.thumbnail-get": (args) => {
          const svg = host.thumbnail(confineRoot(args));
          if (svg === null) throw { message: "no thumbnail" };
          return new TextEncoder().encode(svg);
        },
        "project.read-bytes": (args) => {
          const fs = host.require();
          const path = confine(args["path"]);
          const value = fs.files.get(path);
          if (value === undefined)
            throw { message: `${path}: No such file or directory` };
          return typeof value === "string" ? new TextEncoder().encode(value) : value;
        },
      },
    };
  };
}

/**
 * Test hooks, not part of any real plugin: change the open project's files as
 * if another program had (git pull, Syncthing, another editor), and publish the
 * watcher's `project.changed`. Callable (`kernel.run("dev.write-outside", ...)`)
 * but listed in no command palette.
 */
export function devHooksPlugin(host: MockProjectHost): MockPluginFactory {
  return (emit): MockPlugin => {
    const changed = (
      fs: MockProjectFs,
      changes: readonly { path: string; kind: string; from?: string }[],
    ): void => emit("project.changed", { root: fs.root, changes });
    return {
      name: "dev",
      commands: [],
      handlers: {
        "dev.write-outside": (args) => {
          const fs = host.require();
          const path = confine(args["path"]);
          const content = args["content"];
          if (typeof content !== "string") {
            throw { message: "bad arguments: `content` must be a string" };
          }
          const existed = fs.files.has(path);
          fs.files.set(path, content);
          changed(fs, [{ path, kind: existed ? "modified" : "created" }]);
          return null;
        },
        "dev.delete-outside": (args) => {
          const fs = host.require();
          const path = confine(args["path"]);
          fs.files.delete(path);
          changed(fs, [{ path, kind: "removed" }]);
          return null;
        },
        "dev.rename-outside": (args) => {
          const fs = host.require();
          const from = confine(args["from"], "from");
          const to = confine(args["to"], "to");
          const value = fs.files.get(from);
          if (value === undefined) throw { message: `${from}: No such file` };
          fs.files.delete(from);
          fs.files.set(to, value);
          changed(fs, [{ path: to, kind: "renamed", from }]);
          return null;
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
