/**
 * Markdown to Typst conversion, the parts that need no kernel: where the
 * Typst file goes and what the status line says. Pure, so it is unit-tested.
 */

import { basename, fileKind } from "./paths";

/** `notes.md` -> `notes.typ`, in the same folder; null for a non-Markdown path. */
export function typstPathFor(path: string): string | null {
  if (fileKind(path) !== "markdown") return null;
  const dot = path.lastIndexOf(".");
  const slash = path.lastIndexOf("/");
  return `${dot > slash ? path.slice(0, dot) : path}.typ`;
}

/** What the status line says once converted. */
export function convertedMessage(
  target: string,
  diagnostics: readonly { readonly severity: string }[],
): string {
  const notes = diagnostics.filter((d) => d.severity !== "info").length;
  const name = basename(target);
  if (notes === 0) return `Converted to ${name}.`;
  return `Converted to ${name}; ${notes} ${notes === 1 ? "part" : "parts"} could not be converted exactly.`;
}
