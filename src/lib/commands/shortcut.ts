/**
 * Keyboard shortcuts as data: one definition drives both the global key handler
 * and the text shown beside a command in the palette, so they cannot disagree.
 */

export interface Shortcut {
  /** `KeyboardEvent.key`, compared case-insensitively. */
  readonly key: string;
  /** Ctrl on Windows and Linux, Cmd on macOS. */
  readonly mod?: boolean;
  readonly shift?: boolean;
  readonly alt?: boolean;
}

/** The parts of a `KeyboardEvent` the matcher reads. */
export interface KeyLike {
  readonly key: string;
  readonly ctrlKey: boolean;
  readonly metaKey: boolean;
  readonly shiftKey: boolean;
  readonly altKey: boolean;
}

export function matches(event: KeyLike, shortcut: Shortcut): boolean {
  const mod = event.ctrlKey || event.metaKey;
  return (
    event.key.toLowerCase() === shortcut.key.toLowerCase() &&
    mod === (shortcut.mod ?? false) &&
    event.altKey === (shortcut.alt ?? false) &&
    // Shifted punctuation reports a different `key`, so only letters, Tab and
    // the named keys are checked against Shift.
    event.shiftKey === (shortcut.shift ?? false)
  );
}

export interface Platform {
  readonly isMac: boolean;
}

const NAMES: Record<string, string> = {
  PageDown: "PgDn",
  PageUp: "PgUp",
  ArrowUp: "Up",
  ArrowDown: "Down",
  Escape: "Esc",
};

/** "Ctrl+Shift+P" on Windows and Linux, "⇧⌘P" on macOS. */
export function formatShortcut(shortcut: Shortcut, platform: Platform): string {
  const key = NAMES[shortcut.key] ?? (shortcut.key.length === 1 ? shortcut.key.toUpperCase() : shortcut.key);
  if (platform.isMac) {
    return `${shortcut.alt === true ? "⌥" : ""}${shortcut.shift === true ? "⇧" : ""}${shortcut.mod === true ? "⌘" : ""}${key}`;
  }
  return [
    shortcut.mod === true ? "Ctrl" : null,
    shortcut.alt === true ? "Alt" : null,
    shortcut.shift === true ? "Shift" : null,
    key,
  ]
    .filter((part): part is string => part !== null)
    .join("+");
}
