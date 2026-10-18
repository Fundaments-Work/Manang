/**
 * The figure editor's tools and keyboard: one table drives the toolbar,
 * tooltips, `aria-keyshortcuts`, the key handler and the shortcut overlay,
 * so they cannot disagree.
 */

import type { Shortcut } from "$lib/commands/shortcut";

export type Tool =
  | "select"
  | "hand"
  | "rect"
  | "diamond"
  | "ellipse"
  | "arrow"
  | "line"
  | "draw"
  | "text"
  | "frame"
  | "eraser"
  | "laser"
  | "math";

export interface ToolSpec {
  readonly tool: Tool;
  /** Also the button's accessible name. */
  readonly label: string;
  readonly key: string;
}

/** Toolbar order; 1..9 pick the first nine. */
export const TOOLS: readonly ToolSpec[] = [
  { tool: "select", label: "Select", key: "V" },
  { tool: "hand", label: "Hand", key: "H" },
  { tool: "rect", label: "Rectangle", key: "R" },
  { tool: "diamond", label: "Diamond", key: "D" },
  { tool: "ellipse", label: "Ellipse", key: "O" },
  { tool: "arrow", label: "Arrow", key: "A" },
  { tool: "line", label: "Line", key: "L" },
  { tool: "draw", label: "Draw", key: "P" },
  { tool: "text", label: "Text", key: "T" },
  { tool: "frame", label: "Frame", key: "F" },
  { tool: "eraser", label: "Eraser", key: "E" },
  { tool: "laser", label: "Laser", key: "K" },
  { tool: "math", label: "Formula", key: "M" },
];

/** Extra keys kept from the first editor (B for the old freehand key). */
const ALIASES: Readonly<Record<string, Tool>> = { b: "draw" };

/** The tool a plain key picks, if any. */
export function toolForKey(key: string): Tool | undefined {
  const k = key.toLowerCase();
  const spec = TOOLS.find((t) => t.key.toLowerCase() === k);
  if (spec !== undefined) return spec.tool;
  if (ALIASES[k] !== undefined) return ALIASES[k];
  if (/^[1-9]$/.test(k)) return TOOLS[Number(k) - 1]?.tool;
  return undefined;
}

export type Action =
  | { readonly type: "tool"; readonly tool: Tool }
  | { readonly type: "nudge"; readonly dx: number; readonly dy: number }
  | {
      readonly type:
        | "undo"
        | "redo"
        | "select-all"
        | "duplicate"
        | "group"
        | "ungroup"
        | "lock"
        | "front"
        | "back"
        | "forward"
        | "backward"
        | "zoom-in"
        | "zoom-out"
        | "zoom-reset"
        | "zoom-fit"
        | "zoom-selection"
        | "delete"
        | "escape"
        | "edit";
    };

/** The parts of a keyboard event the editor reads. */
export interface KeyEventLike {
  readonly key: string;
  readonly code?: string;
  readonly ctrlKey: boolean;
  readonly metaKey: boolean;
  readonly shiftKey: boolean;
  readonly altKey: boolean;
}

const NUDGE: Readonly<Record<string, readonly [number, number]>> = {
  ArrowLeft: [-1, 0],
  ArrowRight: [1, 0],
  ArrowUp: [0, -1],
  ArrowDown: [0, 1],
};

/**
 * The editor action a key press means, or undefined (Ctrl+S, Ctrl+C/X/V and
 * anything else pass through to the workbench and the clipboard events).
 */
export function actionFor(e: KeyEventLike): Action | undefined {
  const mod = e.ctrlKey || e.metaKey;
  const key = e.key;
  const lower = key.toLowerCase();
  if (e.altKey) return undefined;
  if (mod) {
    if (lower === "z") return { type: e.shiftKey ? "redo" : "undo" };
    if (lower === "y" && !e.shiftKey) return { type: "redo" };
    if (lower === "a" && !e.shiftKey) return { type: "select-all" };
    if (lower === "d" && !e.shiftKey) return { type: "duplicate" };
    if (lower === "g") return { type: e.shiftKey ? "ungroup" : "group" };
    if (lower === "l" && e.shiftKey) return { type: "lock" };
    if (key === "]" || key === "}") return { type: e.shiftKey ? "front" : "forward" };
    if (key === "[" || key === "{") return { type: e.shiftKey ? "back" : "backward" };
    if (key === "=" || key === "+") return { type: "zoom-in" };
    if (key === "-" || key === "_") return { type: "zoom-out" };
    if (key === "0" && !e.shiftKey) return { type: "zoom-reset" };
    return undefined;
  }
  if (e.shiftKey && (e.code === "Digit1" || key === "!")) return { type: "zoom-fit" };
  if (e.shiftKey && (e.code === "Digit2" || key === "@"))
    return { type: "zoom-selection" };
  const step = NUDGE[key];
  if (step !== undefined) {
    const f = e.shiftKey ? 10 : 1;
    return { type: "nudge", dx: step[0] * f, dy: step[1] * f };
  }
  if (key === "Delete" || key === "Backspace") return { type: "delete" };
  if (key === "Escape") return { type: "escape" };
  if (key === "Enter") return { type: "edit" };
  if (e.shiftKey) return undefined;
  const tool = toolForKey(key);
  return tool === undefined ? undefined : { type: "tool", tool };
}

/** Entries for the shortcut overlay ("Figure editor" group). */
export const FIGURE_SHORTCUTS: readonly {
  readonly id: string;
  readonly title: string;
  readonly shortcut: Shortcut;
  readonly description?: string;
}[] = [
  ...TOOLS.map((t) => ({
    id: `figure.tool.${t.tool}`,
    title: `${t.label} tool`,
    shortcut: { key: t.key },
  })),
  { id: "figure.tools.number", title: "Tool by position", shortcut: { key: "1-9" } },
  { id: "figure.undo", title: "Undo", shortcut: { key: "z", mod: true } },
  { id: "figure.redo", title: "Redo", shortcut: { key: "z", mod: true, shift: true } },
  { id: "figure.select-all", title: "Select all", shortcut: { key: "a", mod: true } },
  { id: "figure.duplicate", title: "Duplicate", shortcut: { key: "d", mod: true } },
  { id: "figure.group", title: "Group", shortcut: { key: "g", mod: true } },
  {
    id: "figure.ungroup",
    title: "Ungroup",
    shortcut: { key: "g", mod: true, shift: true },
  },
  {
    id: "figure.lock",
    title: "Lock or unlock",
    shortcut: { key: "l", mod: true, shift: true },
  },
  { id: "figure.forward", title: "Bring forward", shortcut: { key: "]", mod: true } },
  { id: "figure.backward", title: "Send backward", shortcut: { key: "[", mod: true } },
  {
    id: "figure.front",
    title: "Bring to front",
    shortcut: { key: "]", mod: true, shift: true },
  },
  {
    id: "figure.back",
    title: "Send to back",
    shortcut: { key: "[", mod: true, shift: true },
  },
  { id: "figure.zoom-in", title: "Zoom in", shortcut: { key: "=", mod: true } },
  { id: "figure.zoom-out", title: "Zoom out", shortcut: { key: "-", mod: true } },
  { id: "figure.zoom-reset", title: "Zoom to 100%", shortcut: { key: "0", mod: true } },
  { id: "figure.zoom-fit", title: "Zoom to fit", shortcut: { key: "1", shift: true } },
  {
    id: "figure.zoom-selection",
    title: "Zoom to selection",
    shortcut: { key: "2", shift: true },
  },
  {
    id: "figure.pan",
    title: "Pan the canvas",
    shortcut: { key: "Space" },
    description: "Hold and drag; or drag with the middle button, or scroll",
  },
  { id: "figure.nudge", title: "Nudge (Shift: 10)", shortcut: { key: "Arrows" } },
  {
    id: "figure.edit",
    title: "Edit label",
    shortcut: { key: "Enter" },
    description: "Or double-click a shape",
  },
  { id: "figure.delete", title: "Delete selection", shortcut: { key: "Delete" } },
  {
    id: "figure.no-snap",
    title: "Move without snapping",
    shortcut: { key: "Alt" },
    description: "Hold while dragging",
  },
];
