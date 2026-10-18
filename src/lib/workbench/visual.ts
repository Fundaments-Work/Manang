/**
 * Rules of the Visual editor mode that need no browser or kernel: which files
 * have it, what the per-type default setting means, and where an image source
 * points. Tested in `visual.test.ts`.
 */

import { dirname, extension } from "./paths";

export type VisualFormat = "markdown" | "typst";

/** The format whose adapter serves this file, or null (no Visual mode). */
export function visualFormat(path: string): VisualFormat | null {
  switch (extension(path)) {
    case "md":
    case "markdown":
      return "markdown";
    case "typ":
      return "typst";
    default:
      return null;
  }
}

/** The setting that opens files of a type in Visual: a list of format ids. */
export const VISUAL_DEFAULT_SETTING = "editor.visual-default";

/** Whether the setting's value lists `format`. Anything else reads as off. */
export function opensVisual(value: unknown, format: VisualFormat): boolean {
  return Array.isArray(value) && value.includes(format);
}

/** The setting's new value after turning `format` on or off. */
export function withVisualDefault(
  value: unknown,
  format: VisualFormat,
  on: boolean,
): VisualFormat[] {
  const current = (Array.isArray(value) ? value : []).filter(
    (v): v is VisualFormat => v === "markdown" || v === "typst",
  );
  const rest = current.filter((f) => f !== format);
  return on ? [...rest, format].sort() : rest;
}

/** Resolve `.` and `..` segments of a project path; null if it leaves the project. */
function normalize(path: string): string | null {
  const out: string[] = [];
  for (const part of path.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === "..") {
      if (out.pop() === undefined) return null;
    } else out.push(part);
  }
  return out.join("/");
}

/**
 * The project path an image `src` written in the document `docPath` stands for:
 * relative to the document, or to the project when it starts with `/` (Typst).
 * A URL, a data URI or a path leaving the project is not a project file.
 */
export function resolveAsset(docPath: string, src: string): string | null {
  const text = src.trim();
  if (text === "" || /^[a-z][a-z0-9+.-]*:/i.test(text) || text.startsWith("//")) {
    return null;
  }
  const clean = text.split(/[?#]/)[0] ?? "";
  return normalize(clean.startsWith("/") ? clean : `${dirname(docPath)}/${clean}`);
}

/** The figure scene behind an exported SVG (`x.svg` beside `x.figure`), or null. */
export function figureSceneFor(
  svgPath: string,
  scenes: ReadonlySet<string>,
): string | null {
  if (!svgPath.endsWith(".svg")) return null;
  const stem = svgPath.slice(0, -".svg".length);
  for (const candidate of [`${stem}.figure`, `${stem}.figure.json`]) {
    if (scenes.has(candidate)) return candidate;
  }
  return null;
}
