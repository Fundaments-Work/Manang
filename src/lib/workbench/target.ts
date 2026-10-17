/**
 * Which file a preview compiles, and when a recompile can be skipped.
 *
 * A chapter pulled in with `#include` / `\input` is not a document of its own:
 * compiling it alone fails or shows a fragment. So a file that belongs to the
 * project's main document is compiled through the main file instead.
 */

import type { Engine } from "$lib/kernel/types";

import { basename, dirname, engineOf } from "./paths";

export interface TargetInput {
  /** The active document's path, or null when there is none (a figure, no tab). */
  readonly active: string | null;
  readonly project: { readonly engine: Engine; readonly main: string } | null;
  /** Text of the main file when known (open buffer or disk); enables include detection. */
  readonly mainText?: string | null;
  /** Text of the active document, to spot a LaTeX root (`\documentclass`). */
  readonly activeText?: string | null;
}

export interface Target {
  readonly engine: Engine;
  /** Project-relative path of the file to compile or render. */
  readonly main: string;
  /** True when the active file is compiled through the project's main file. */
  readonly viaMain: boolean;
}

/** Paths `main` could use to name `path` from `from`'s directory. */
function spellings(path: string, from: string): string[] {
  const base = dirname(from);
  const relative =
    base !== "" && path.startsWith(`${base}/`) ? path.slice(base.length + 1) : path;
  return [...new Set([path, relative, `/${path}`])];
}

/** Does `text` (the main file) include, import or input `path`? */
export function includes(
  engine: Engine,
  text: string,
  path: string,
  mainPath: string,
): boolean {
  const names = spellings(path, mainPath);
  if (engine === "typst") {
    for (const m of text.matchAll(/#(?:include|import)\s*"([^"]+)"/g)) {
      if (names.includes(m[1] ?? "")) return true;
    }
    return false;
  }
  if (engine === "latex") {
    const candidates = new Set(names.flatMap((n) => [n, n.replace(/\.tex$/i, "")]));
    for (const m of text.matchAll(/\\(?:input|include|subfile)\s*\{([^}]+)\}/g)) {
      if (candidates.has((m[1] ?? "").trim())) return true;
    }
  }
  return false;
}

/**
 * The file the preview should show for the active tab, or null when nothing
 * previews (no project and no engine file).
 */
export function resolveTarget(input: TargetInput): Target | null {
  const { active, project } = input;
  const activeEngine = active === null ? null : engineOf(active);

  if (active !== null && activeEngine !== null) {
    const own: Target = { engine: activeEngine, main: active, viaMain: false };
    if (project === null || activeEngine !== project.engine) return own;
    if (active === project.main) return own;
    // Markdown has no includes: every page previews itself.
    if (activeEngine === "markdown") return own;
    if (
      activeEngine === "latex" &&
      input.activeText != null &&
      /\\documentclass\b/.test(input.activeText)
    ) {
      return own;
    }
    const listed =
      input.mainText != null &&
      includes(activeEngine, input.mainText, active, project.main);
    // A LaTeX file without `\documentclass` can only be a chapter.
    if (listed || activeEngine === "latex") {
      return { engine: activeEngine, main: project.main, viaMain: true };
    }
    return own;
  }
  if (project === null) return null;
  return { engine: project.engine, main: project.main, viaMain: false };
}

/** Basename without extension, for download names. */
export function documentName(path: string): string {
  const name = basename(path);
  const dot = name.lastIndexOf(".");
  return dot <= 0 ? name : name.slice(0, dot);
}

/**
 * Replace (or add) `main` under `[project]` in a `manang.toml`, leaving every
 * other line, comment and table exactly as it was.
 */
export function setMainInToml(toml: string, main: string): string {
  const value = `main = ${JSON.stringify(main)}`;
  const eol = toml.includes("\r\n") ? "\r\n" : "\n";
  const lines = toml.split(/\r?\n/);
  let inProject = false;
  let projectAt = -1;
  let lastKey = -1;
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i] ?? "";
    const table = /^\s*\[([^\]]*)\]\s*(?:#.*)?$/.exec(line);
    if (table !== null) {
      if (inProject) break;
      inProject = (table[1] ?? "").trim() === "project";
      if (inProject) {
        projectAt = i;
        lastKey = i;
      }
      continue;
    }
    if (!inProject) continue;
    if (/^\s*main\s*=/.test(line)) {
      const comment = /(\s+#.*)$/.exec(line)?.[1] ?? "";
      lines[i] = value + comment;
      return lines.join(eol);
    }
    if (line.trim() !== "" && !line.trim().startsWith("#")) lastKey = i;
  }
  if (projectAt === -1) {
    const body = toml === "" || toml.endsWith("\n") ? toml : toml + eol;
    return `${body}${body === "" ? "" : eol}[project]${eol}${value}${eol}`;
  }
  lines.splice(lastKey + 1, 0, value);
  return lines.join(eol);
}

/** A cheap, stable fingerprint of a string (FNV-1a), for compile cache keys. */
export function fingerprint(text: string): string {
  let hash = 0x811c9dc5;
  for (let i = 0; i < text.length; i += 1) {
    hash ^= text.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return `${text.length.toString(36)}:${hash.toString(36)}`;
}

/**
 * The inputs a compile depends on. Equal keys give an equal result, so a
 * recompile can be skipped (revision-based cache). `disk` counts file-change
 * events, which is what disk-based engines (LaTeX) depend on.
 */
export function inputKey(
  engine: Engine,
  main: string,
  sources: Readonly<Record<string, string>>,
  disk: number,
): string {
  const parts = Object.keys(sources)
    .sort()
    .map((path) => `${path}=${fingerprint(sources[path] ?? "")}`);
  return [engine, main, engine === "latex" ? `disk${disk}` : "", ...parts].join("|");
}
