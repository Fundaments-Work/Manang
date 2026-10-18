/**
 * Moving around the PDF: the page box in the header and the keys the viewer
 * answers to when it has focus. Pure.
 */

/**
 * The 0-based page for what was typed in the page box ("12", " 12 ",
 * "12/40"), clamped to the document; null when it is not a page number.
 */
export function parsePageInput(text: string, count: number): number | null {
  if (count <= 0) return null;
  const match = /^\s*(\d{1,7})\s*(?:\/\s*\d*\s*)?$/.exec(text);
  if (match === null || match[1] === undefined) return null;
  const typed = Number(match[1]);
  if (!Number.isInteger(typed) || typed < 1) return null;
  return Math.min(typed, count) - 1;
}

export type NavAction =
  | { readonly kind: "page"; readonly delta: number }
  | { readonly kind: "first" }
  | { readonly kind: "last" }
  | { readonly kind: "scroll"; readonly dx: number; readonly dy: number }
  | { readonly kind: "screen"; readonly direction: 1 | -1 };

/** Pixels an arrow key scrolls. */
export const ARROW_STEP = 48;

/** What a key does in the focused viewer, or null to let it through. */
export function navigationKey(event: {
  readonly key: string;
  readonly ctrlKey: boolean;
  readonly metaKey: boolean;
  readonly altKey: boolean;
  readonly shiftKey: boolean;
}): NavAction | null {
  if (event.altKey) return null;
  const mod = event.ctrlKey || event.metaKey;
  switch (event.key) {
    case "PageDown":
      return mod ? null : { kind: "page", delta: 1 };
    case "PageUp":
      return mod ? null : { kind: "page", delta: -1 };
    case "Home":
      return { kind: "first" };
    case "End":
      return { kind: "last" };
    case "ArrowDown":
      return mod ? null : { kind: "scroll", dx: 0, dy: ARROW_STEP };
    case "ArrowUp":
      return mod ? null : { kind: "scroll", dx: 0, dy: -ARROW_STEP };
    case "ArrowRight":
      return mod ? null : { kind: "scroll", dx: ARROW_STEP, dy: 0 };
    case "ArrowLeft":
      return mod ? null : { kind: "scroll", dx: -ARROW_STEP, dy: 0 };
    case " ":
      return mod ? null : { kind: "screen", direction: event.shiftKey ? -1 : 1 };
    default:
      return null;
  }
}
