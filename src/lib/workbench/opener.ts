/**
 * What opens a file: the one decision every way of opening a file (Files, Go
 * to file, Search, Problems, restored tabs) goes through. Pure, so it is unit
 * tested and no component decides on its own.
 *
 * The name decides when it can. A file whose extension says nothing (no
 * extension, or one not listed here) is decided by its first bytes: valid
 * UTF-8 with no NUL is text, anything else is binary. Data-like text formats
 * (CSV, logs, JSON...) are also probed, only for their size, because they are
 * the files that grow past what an editor tab should load.
 */

import { basename, extension } from "./paths";

/** How a file opens. */
export type OpenAs =
  /** The figure editor (`*.figure.json`). */
  | "figure"
  /** The text editor, highlighted where a language mode exists. */
  | "text"
  /** Text too large to edit: a read-only preview with a notice. */
  | "large-text"
  | "image"
  | "pdf"
  /** Anything else: an info tab with "Open with system app". */
  | "binary";

/** Text above this size opens read-only instead of in the editor. */
export const LARGE_TEXT_BYTES = 5 * 1024 * 1024;

/** How much of an unknown file is examined. */
export const SNIFF_BYTES = 8 * 1024;

/** How much of a large text file the read-only preview shows. */
export const PREVIEW_BYTES = 256 * 1024;

/** Text the editor opens straight away: manuscripts and small config. */
const TEXT = new Set([
  "typ",
  "tex",
  "sty",
  "cls",
  "dtx",
  "ltx",
  "bib",
  "biblatex",
  "bst",
  "md",
  "markdown",
  "toml",
  "yaml",
  "yml",
  "ini",
  "cfg",
  "conf",
  "html",
  "htm",
  "css",
  "js",
  "mjs",
  "cjs",
  "ts",
  "tsx",
  "jsx",
  "svelte",
  "py",
  "rs",
  "sh",
  "bash",
  "zsh",
  "fish",
  "r",
  "jl",
  "lua",
  "c",
  "h",
  "cpp",
  "hpp",
  "go",
  "java",
  "rb",
  "pl",
  "dot",
  // Diagram sources, previewed by the `diagram` plugin.
  "mmd",
  "mermaid",
  "pikchr",
  "bob",
  "gitignore",
  "editorconfig",
  "lock",
]);

/** Text that is often data, and so can be large: probed for its size. */
const DATA_TEXT = new Set(["txt", "csv", "tsv", "json", "jsonl", "xml", "log", "ris"]);

const IMAGES = new Set([
  "png",
  "jpg",
  "jpeg",
  "gif",
  "webp",
  "svg",
  "bmp",
  "avif",
  "ico",
]);

/** Formats that are never text, so their bytes need not be read to decide. */
const BINARY = new Set([
  "zip",
  "gz",
  "tgz",
  "bz2",
  "xz",
  "7z",
  "rar",
  "tar",
  "doc",
  "docx",
  "xls",
  "xlsx",
  "ppt",
  "pptx",
  "odt",
  "ods",
  "odp",
  "epub",
  "woff",
  "woff2",
  "ttf",
  "otf",
  "eot",
  "mp3",
  "wav",
  "ogg",
  "flac",
  "m4a",
  "aac",
  "mp4",
  "mov",
  "mkv",
  "webm",
  "avi",
  "wasm",
  "exe",
  "dll",
  "so",
  "dylib",
  "bin",
  "dat",
  "db",
  "sqlite",
  "pyc",
  "class",
  "jar",
  "psd",
  "tif",
  "tiff",
  "heic",
  "eps",
  "dvi",
  "synctex",
]);

/** The decision the name alone gives, or null when the bytes must decide. */
export function openAsByName(path: string): OpenAs | null {
  const name = basename(path).toLowerCase();
  if (name.endsWith(".figure.json")) return "figure";
  const ext = extension(path);
  if (ext === "pdf") return "pdf";
  if (IMAGES.has(ext)) return "image";
  if (BINARY.has(ext)) return "binary";
  if (TEXT.has(ext)) return "text";
  return null;
}

/** Whether opening `path` needs its bytes first (to sniff or to size it). */
export function needsProbe(path: string): boolean {
  return openAsByName(path) === null || DATA_TEXT.has(extension(path));
}

/** Valid UTF-8 with no NUL byte. A sequence cut at the end of `head` is fine. */
export function looksLikeText(head: Uint8Array): boolean {
  if (head.includes(0)) return false;
  try {
    new TextDecoder("utf-8", { fatal: true }).decode(head, { stream: true });
    return true;
  } catch {
    return false;
  }
}

/** The decision for a probed file: its name if it says text, else its first bytes. */
export function openAsByContent(path: string, bytes: Uint8Array): OpenAs {
  const named = openAsByName(path);
  if (named !== null && named !== "text") return named;
  const text =
    named === "text" ||
    DATA_TEXT.has(extension(path)) ||
    looksLikeText(bytes.subarray(0, SNIFF_BYTES));
  if (!text) return "binary";
  return bytes.length > LARGE_TEXT_BYTES ? "large-text" : "text";
}

/** Tab kinds: the editor, the figure editor, or a read-only viewer. */
export function tabKindOf(as: OpenAs): "document" | "figure" | "viewer" {
  if (as === "figure") return "figure";
  return as === "text" ? "document" : "viewer";
}

const DESCRIPTIONS: Readonly<Record<string, string>> = {
  zip: "ZIP archive",
  gz: "Gzip archive",
  tgz: "Gzip archive",
  tar: "Tar archive",
  "7z": "7-Zip archive",
  rar: "RAR archive",
  doc: "Word document",
  docx: "Word document",
  xls: "Excel spreadsheet",
  xlsx: "Excel spreadsheet",
  ppt: "PowerPoint presentation",
  pptx: "PowerPoint presentation",
  odt: "OpenDocument text",
  ods: "OpenDocument spreadsheet",
  odp: "OpenDocument presentation",
  epub: "EPUB book",
  woff: "Web font",
  woff2: "Web font",
  ttf: "TrueType font",
  otf: "OpenType font",
  mp3: "MP3 audio",
  wav: "WAV audio",
  ogg: "Ogg audio",
  flac: "FLAC audio",
  m4a: "AAC audio",
  mp4: "MP4 video",
  mov: "QuickTime video",
  mkv: "Matroska video",
  webm: "WebM video",
  avi: "AVI video",
  eps: "Encapsulated PostScript",
  dvi: "DVI document",
  tif: "TIFF image",
  tiff: "TIFF image",
  psd: "Photoshop image",
  heic: "HEIC image",
  sqlite: "SQLite database",
  db: "Database",
  wasm: "WebAssembly module",
};

/** A short type name for the info tab ("ZIP archive", "Binary file"). */
export function describeFile(path: string): string {
  const ext = extension(path);
  if (Object.hasOwn(DESCRIPTIONS, ext)) return DESCRIPTIONS[ext] ?? "Binary file";
  return ext === "" ? "Binary file" : `${ext.toUpperCase()} file`;
}

/** Bytes as a short size ("812 B", "3.4 KB", "12.0 MB"). */
export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/**
 * A modification time (ms since the epoch) as a local date and time
 * ("2026-10-18 14:05"), or "" when unknown.
 */
export function formatModified(ms: number | null): string {
  if (ms === null || !Number.isFinite(ms)) return "";
  const d = new Date(ms);
  const two = (n: number): string => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())} ${two(d.getHours())}:${two(d.getMinutes())}`;
}
