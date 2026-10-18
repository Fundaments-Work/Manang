/**
 * What a figure file is called. A figure is a JSON scene saved as
 * `<name>.figure`, next to its rendered `<name>.svg`. Scenes written before
 * 2026-10-18 end in `.figure.json`: they still open and save in place under
 * their own name (no second file appears), and everything that recognises a
 * figure goes through here so the two suffixes are handled in one place.
 */

import type { DialogRequest } from "$lib/workbench/dialogRequest";

export const FIGURE_EXTENSION = ".figure";
export const LEGACY_FIGURE_SUFFIX = ".figure.json";

/** Whether a file name (not a path) is a figure scene. */
export function isFigureName(name: string): boolean {
  const lower = name.toLowerCase();
  return lower.endsWith(FIGURE_EXTENSION) || lower.endsWith(LEGACY_FIGURE_SUFFIX);
}

/** The name without its figure suffix (`a.figure` and `a.figure.json` give `a`). */
export function figureStem(name: string): string {
  const lower = name.toLowerCase();
  if (lower.endsWith(LEGACY_FIGURE_SUFFIX))
    return name.slice(0, -LEGACY_FIGURE_SUFFIX.length);
  if (lower.endsWith(FIGURE_EXTENSION)) return name.slice(0, -FIGURE_EXTENSION.length);
  return name;
}

/** The name a new figure starts with. */
export const NEW_FIGURE_NAME = `untitled${FIGURE_EXTENSION}`;

/** Why a typed figure name cannot be used, or null when it can. */
export function figureNameProblem(name: string): string | null {
  const stem = name.trim();
  if (stem === "") return "Give the figure a name.";
  if (/[\\/]/.test(stem)) return "A name cannot contain a slash.";
  if (stem.startsWith(".")) return "A name cannot start with a dot.";
  return null;
}

/** The New figure dialog: just a name, one Create. */
export function newFigureRequest(name: string): DialogRequest {
  return {
    title: "New figure",
    message:
      "The figure is saved in figures/ as a .figure file, with its picture (.svg) next to it for your documents.",
    input: { label: "Name", value: name },
    actions: [{ id: "create", label: "Create" }],
  };
}
