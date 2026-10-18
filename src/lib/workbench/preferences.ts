/**
 * From settings values to what the shell applies: the theme, the interface
 * scale, motion, the editor's text and gutter, and how eagerly the preview
 * recompiles.
 *
 * Pure so the rules are testable and so a malformed settings file can never
 * hand the editor a font size of zero: every number is clamped to the range
 * the settings schema declares, and anything unexpected falls back to the
 * default.
 */

export type ThemeChoice = "dark" | "light" | "system";
export type MotionChoice = "system" | "on" | "off";

export interface EditorPrefs {
  readonly fontSize: number;
  readonly lineHeight: number;
  readonly tabSize: number;
  readonly wordWrap: boolean;
  readonly lineNumbers: boolean;
}

export interface PreviewPrefs {
  /** Recompile while typing; when off only saves and file switches compile. */
  readonly autoCompile: boolean;
  /** Wait after the last keystroke before recompiling. */
  readonly debounceMs: number;
  /** Double-click on a PDF page goes to the source (Alt+double-click selects). */
  readonly doubleClickSource: boolean;
  /** Show PDF pages inverted, hue preserved. */
  readonly darkPages: boolean;
}

export interface Preferences {
  readonly theme: ThemeChoice;
  readonly uiScale: number;
  readonly reducedMotion: MotionChoice;
  readonly editor: EditorPrefs;
  readonly preview: PreviewPrefs;
}

export const DEFAULT_PREFERENCES: Preferences = {
  theme: "dark",
  uiScale: 1,
  reducedMotion: "system",
  editor: {
    fontSize: 14,
    lineHeight: 1.6,
    tabSize: 2,
    wordWrap: true,
    lineNumbers: true,
  },
  preview: {
    autoCompile: true,
    debounceMs: 300,
    doubleClickSource: true,
    darkPages: false,
  },
};

type Lookup = (id: string) => unknown;

function num(value: unknown, fallback: number, min: number, max: number): number {
  if (typeof value !== "number" || !Number.isFinite(value)) return fallback;
  return Math.min(max, Math.max(min, value));
}

function bool(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

function choice<T extends string>(
  value: unknown,
  options: readonly T[],
  fallback: T,
): T {
  return typeof value === "string" && (options as readonly string[]).includes(value)
    ? (value as T)
    : fallback;
}

/** Read the preferences out of effective settings values. */
export function resolvePreferences(get: Lookup): Preferences {
  const d = DEFAULT_PREFERENCES;
  return {
    theme: choice(get("appearance.theme"), ["dark", "light", "system"], d.theme),
    uiScale: num(get("appearance.ui-scale"), d.uiScale, 0.75, 2),
    reducedMotion: choice(
      get("appearance.reduced-motion"),
      ["system", "on", "off"],
      d.reducedMotion,
    ),
    editor: {
      fontSize: Math.round(num(get("editor.font-size"), d.editor.fontSize, 8, 48)),
      lineHeight: num(get("editor.line-height"), d.editor.lineHeight, 1, 3),
      tabSize: Math.round(num(get("editor.tab-size"), d.editor.tabSize, 1, 16)),
      wordWrap: bool(get("editor.word-wrap"), d.editor.wordWrap),
      lineNumbers: bool(get("editor.line-numbers"), d.editor.lineNumbers),
    },
    preview: {
      autoCompile: bool(get("preview.auto-compile"), d.preview.autoCompile),
      debounceMs: Math.round(
        num(get("preview.debounce-ms"), d.preview.debounceMs, 0, 5000),
      ),
      doubleClickSource: bool(
        get("preview.double-click-source"),
        d.preview.doubleClickSource,
      ),
      darkPages: bool(get("preview.dark-pages"), d.preview.darkPages),
    },
  };
}

/** The concrete theme for a choice; `system` asks the operating system. */
export function effectiveTheme(
  theme: ThemeChoice,
  prefersLight: boolean,
): "dark" | "light" {
  if (theme === "system") return prefersLight ? "light" : "dark";
  return theme;
}

/** Whether to drop transitions, given the choice and the OS preference. */
export function motionReduced(choice: MotionChoice, systemReduced: boolean): boolean {
  return choice === "system" ? systemReduced : choice === "on";
}
