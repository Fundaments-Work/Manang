/**
 * The Visual toolbar as data: which buttons exist for a format, their order,
 * labels and shortcuts, and how command state turns into pressed/disabled.
 * Pure, so the rules are tested without the browser.
 */

import type { CommandState } from "$lib/editor/visual/protocol";
import { dirname } from "./paths";
import type { VisualFormat } from "./visual";

export type ToolbarIcon =
  | "bold"
  | "italic"
  | "code"
  | "strike"
  | "heading"
  | "list-bullet"
  | "list-ordered"
  | "quote"
  | "link"
  | "image"
  | "formula"
  | "rule";

export interface ToolbarItem {
  /** Stable id, also the `data-visual` attribute for tests. */
  readonly id: string;
  /** The adapter command it runs (the `visual.<command>` toolbar command). */
  readonly command: string;
  readonly icon: ToolbarIcon;
  readonly label: string;
  /** A fixed argument (`bullet`), if any. */
  readonly args?: string;
  /** Which kind of control it is. */
  readonly control: "button" | "heading" | "link" | "insert" | "math";
  /** Shortcut in registry form (`Mod+B`), shown in the tooltip. */
  readonly shortcut?: string;
  /** Start a new visual group (a hairline before it). */
  readonly groupStart?: boolean;
}

export const TOOLBAR: readonly ToolbarItem[] = [
  {
    id: "bold",
    command: "bold",
    icon: "bold",
    label: "Bold",
    control: "button",
    shortcut: "Mod+B",
  },
  {
    id: "italic",
    command: "italic",
    icon: "italic",
    label: "Italic",
    control: "button",
    shortcut: "Mod+I",
  },
  { id: "code", command: "code", icon: "code", label: "Code", control: "button" },
  {
    id: "strike",
    command: "strike",
    icon: "strike",
    label: "Strikethrough",
    control: "button",
  },
  {
    id: "heading",
    command: "heading",
    icon: "heading",
    label: "Heading",
    control: "heading",
    groupStart: true,
  },
  {
    id: "bullets",
    command: "list",
    icon: "list-bullet",
    label: "Bulleted list",
    args: "bullet",
    control: "button",
    groupStart: true,
  },
  {
    id: "numbers",
    command: "list",
    icon: "list-ordered",
    label: "Numbered list",
    args: "ordered",
    control: "button",
  },
  { id: "quote", command: "quote", icon: "quote", label: "Quote", control: "button" },
  {
    id: "link",
    command: "link",
    icon: "link",
    label: "Link",
    control: "link",
    groupStart: true,
  },
  {
    id: "image",
    command: "image",
    icon: "image",
    label: "Insert image or figure",
    control: "insert",
  },
  { id: "math", command: "math", icon: "formula", label: "Formula", control: "math" },
  {
    id: "rule",
    command: "rule",
    icon: "rule",
    label: "Horizontal rule",
    control: "button",
  },
];

/**
 * The items to show for a format. `states` is what `visual.command-state`
 * returned (empty while unknown: everything shows). An adapter lists only what
 * it implements, so a command absent from a non-empty list is not offered.
 * Typst inserts a figure (with a caption), Markdown an image.
 */
export function itemsFor(
  format: VisualFormat,
  states: readonly CommandState[],
): ToolbarItem[] {
  const known = new Set(states.map((s) => s.command));
  const out: ToolbarItem[] = [];
  let pendingGroup = false;
  for (const item of TOOLBAR) {
    const command =
      item.id === "image" &&
      format === "typst" &&
      (states.length === 0 || known.has("figure"))
        ? "figure"
        : item.command;
    if (states.length > 0 && !known.has(command)) {
      if (item.groupStart === true) pendingGroup = true;
      continue;
    }
    const startsGroup = item.groupStart === true || pendingGroup;
    pendingGroup = false;
    out.push({
      ...item,
      command,
      ...(startsGroup && out.length > 0 ? { groupStart: true } : { groupStart: false }),
    });
  }
  return out;
}

export function stateOf(
  states: readonly CommandState[],
  command: string,
): CommandState | undefined {
  return states.find((s) => s.command === command);
}

/** Whether a command's button is disabled: only when the host says it cannot run. */
export function isDisabled(states: readonly CommandState[], command: string): boolean {
  const state = stateOf(states, command);
  return state !== undefined && !state.enabled;
}

/** Whether a command's button is pressed. A list button is pressed only for its own type. */
export function isPressed(
  states: readonly CommandState[],
  item: ToolbarItem,
  headingLevel: number,
): boolean {
  if (item.control === "heading") return headingLevel > 0;
  return stateOf(states, item.command)?.active === true;
}

/** The levels offered by the heading menu, 0 being plain text. */
export const HEADING_LEVELS: readonly { level: number; label: string }[] = [
  { level: 0, label: "Paragraph" },
  { level: 1, label: "Heading 1" },
  { level: 2, label: "Heading 2" },
  { level: 3, label: "Heading 3" },
  { level: 4, label: "Heading 4" },
  { level: 5, label: "Heading 5" },
  { level: 6, label: "Heading 6" },
];

/** The argument of `heading` for a choice: choosing the current level clears it. */
export function headingArg(current: number, chosen: number): string {
  return String(chosen === current ? 0 : chosen);
}

/** Text of a tooltip: the label with its shortcut, e.g. `Bold (Ctrl+B)`. */
export function tooltip(label: string, shortcut: string | undefined): string {
  return shortcut === undefined || shortcut === "" ? label : `${label} (${shortcut})`;
}

/** `target` written relative to the folder of `docPath`, as an image source. */
export function relativeTo(docPath: string, target: string): string {
  const from = dirname(docPath)
    .split("/")
    .filter((p) => p !== "");
  const to = target.split("/");
  let shared = 0;
  while (
    shared < from.length &&
    shared < to.length - 1 &&
    from[shared] === to[shared]
  ) {
    shared += 1;
  }
  const up = from.slice(shared).map(() => "..");
  return [...up, ...to.slice(shared)].join("/");
}
