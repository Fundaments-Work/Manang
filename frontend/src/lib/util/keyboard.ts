/**
 * Platform-correct keyboard helpers.
 *
 * DESIGN.md specifies Cmd shortcuts throughout, but Manang ships on Linux and
 * Windows too, where the same gesture is Ctrl. Showing "Cmd+K" to a Linux user
 * is a small lie that makes the product feel ported rather than built.
 */

export interface Platform {
  readonly isMac: boolean;
  readonly mod: string;
  readonly alt: string;
}

export function detectPlatform(): Platform {
  const isMac =
    typeof navigator !== "undefined" &&
    /mac|iphone|ipad/i.test(navigator.platform || navigator.userAgent);
  return {
    isMac,
    mod: isMac ? "⌘" : "Ctrl",
    alt: isMac ? "⌥" : "Alt",
  };
}

/** True when the event carries the platform's primary modifier. */
export function hasMod(event: KeyboardEvent): boolean {
  return event.metaKey || event.ctrlKey;
}

/** True when the event should be treated as a plain keystroke, not a shortcut. */
export function isPlainKey(event: KeyboardEvent): boolean {
  return !event.metaKey && !event.ctrlKey && !event.altKey;
}

/**
 * True when the keystroke belongs to whatever has focus.
 *
 * Global shortcuts must stand down when the author is typing, or when the caret
 * sits in the manuscript and an arrow key means "move the caret" rather than
 * "next slide". Without this check an overlay steals the editor's navigation.
 */
export function isTextEntry(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;

  if (target.tagName === "INPUT" || target.tagName === "TEXTAREA") return true;
  if (target.isContentEditable) return true;

  // CodeMirror renders contenteditable spans rather than a form control.
  return target.closest(".cm-editor") !== null;
}
