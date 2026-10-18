/**
 * Project-relative path helpers and file classification. Paths are always
 * `/`-separated and relative to the project root, as the kernel reports them.
 */

export type FileKind =
  | "typst"
  | "latex"
  | "markdown"
  | "bibliography"
  | "figure"
  | "image"
  | "config"
  | "text"
  | "binary";

export type Engine = "typst" | "latex" | "markdown";

export function basename(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

export function dirname(path: string): string {
  const at = path.lastIndexOf("/");
  return at === -1 ? "" : path.slice(0, at);
}

export function join(dir: string, name: string): string {
  return dir === "" ? name : `${dir}/${name}`;
}

/** True when `path` is `ancestor` itself or lives below it. */
export function isUnder(path: string, ancestor: string): boolean {
  return ancestor === "" || path === ancestor || path.startsWith(`${ancestor}/`);
}

/** Re-root `path` after `from` was renamed to `to`; unrelated paths pass through. */
export function moved(path: string, from: string, to: string): string {
  if (path === from) return to;
  return path.startsWith(`${from}/`) ? to + path.slice(from.length) : path;
}

export function extension(path: string): string {
  const name = basename(path);
  const dot = name.lastIndexOf(".");
  return dot <= 0 ? "" : name.slice(dot + 1).toLowerCase();
}

const BINARY = new Set([
  "pdf",
  "png",
  "jpg",
  "jpeg",
  "gif",
  "webp",
  "ico",
  "zip",
  "gz",
  "woff",
  "woff2",
  "ttf",
  "otf",
  "wasm",
]);

export function fileKind(path: string): FileKind {
  const name = basename(path).toLowerCase();
  if (name.endsWith(".figure.json")) return "figure";
  if (name === "manang.toml") return "config";
  switch (extension(path)) {
    case "typ":
      return "typst";
    case "tex":
      return "latex";
    case "md":
    case "markdown":
      return "markdown";
    case "bib":
      return "bibliography";
    case "svg":
      return "image";
    default:
      return BINARY.has(extension(path)) ? "binary" : "text";
  }
}

/** Media types for the images a viewer tab shows (see `opener.ts`). */
const IMAGE_TYPES: Readonly<Record<string, string>> = {
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  webp: "image/webp",
  svg: "image/svg+xml",
  bmp: "image/bmp",
  avif: "image/avif",
  ico: "image/x-icon",
};

/** The media type of an image a viewer can show, or null. */
export function imageType(path: string): string | null {
  const ext = extension(path);
  return Object.hasOwn(IMAGE_TYPES, ext) ? (IMAGE_TYPES[ext] ?? null) : null;
}

/** The engine a document is written for, or null for non-manuscript files. */
export function engineOf(path: string): Engine | null {
  const kind = fileKind(path);
  return kind === "typst" || kind === "latex" || kind === "markdown" ? kind : null;
}

/** Why `name` cannot be used for a new or renamed entry, or null when it can. */
export function nameProblem(
  name: string,
  existing: Iterable<string>,
  current?: string,
): string | null {
  const siblings = new Set(existing);
  const trimmed = name.trim();
  if (trimmed === "") return "Enter a name.";
  if (trimmed === "." || trimmed === "..") return "That name is reserved.";
  if (/[\\/]/.test(trimmed)) return "Names cannot contain slashes.";
  if (trimmed !== current && siblings.has(trimmed)) {
    return `"${trimmed}" already exists in this folder.`;
  }
  return null;
}

/** `untitled.typ`, then `untitled 2.typ`, … the first name not in `taken`. */
export function uniqueName(base: string, existing: Iterable<string>): string {
  const taken = new Set(existing);
  if (!taken.has(base)) return base;
  const dot = base.lastIndexOf(".");
  const stem = dot <= 0 ? base : base.slice(0, dot);
  const ext = dot <= 0 ? "" : base.slice(dot);
  for (let n = 2; ; n += 1) {
    const candidate = `${stem} ${n}${ext}`;
    if (!taken.has(candidate)) return candidate;
  }
}

/** Default new-file name for a project's engine. */
export function newFileName(engine: Engine): string {
  return engine === "typst"
    ? "untitled.typ"
    : engine === "latex"
      ? "untitled.tex"
      : "untitled.md";
}
