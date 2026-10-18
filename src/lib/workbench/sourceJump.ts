/**
 * Pure helpers for source and preview jumps (see `state/sourceJump.ts`):
 * path conversions for SyncTeX, the box marked around a Typst position, and
 * the page size of a rendered SVG. Points throughout, top-left origin.
 */

import type { PagePoint } from "$lib/kernel/api";

import { dirname, join } from "./paths";

/** Height of the band marked around a Typst text position (points). */
const TEXT_BAND = 14;

/**
 * Project-relative path of a SyncTeX `file`, which is relative to the main
 * file's directory unless it lies outside it (then absolute). Null when it is
 * outside the project (a class or package file of the TeX installation).
 */
export function latexSourcePath(
  file: string,
  target: string,
  root: string,
): string | null {
  if (file.startsWith("/")) {
    const prefix = `${root.replace(/\/+$/, "")}/`;
    return root !== "" && file.startsWith(prefix) ? file.slice(prefix.length) : null;
  }
  const parts: string[] = [];
  for (const part of join(dirname(target), file).split("/")) {
    if (part === "" || part === ".") continue;
    if (part === "..") {
      if (parts.pop() === undefined) return null;
    } else parts.push(part);
  }
  return parts.join("/");
}

/** `path` (project-relative) as SyncTeX names it: relative to the main file's directory. */
export function synctexFile(path: string, target: string, root: string): string {
  const base = dirname(target);
  if (base === "") return path;
  return path.startsWith(`${base}/`)
    ? path.slice(base.length + 1)
    : join(root.replace(/\/+$/, ""), path);
}

/** A box around a Typst text position (its baseline start) on a page `pageWidth` wide. */
export function typstMarkBox(
  point: PagePoint,
  pageWidth: number | null,
): { page: number; x: number; y: number; width: number; height: number } {
  const x = Math.max(0, point.x - 2);
  const width =
    pageWidth === null
      ? 160
      : Math.max(24, pageWidth - point.x - Math.min(point.x, 72));
  return {
    page: point.page,
    x,
    y: Math.max(0, point.y - TEXT_BAND + 3),
    width,
    height: TEXT_BAND,
  };
}
