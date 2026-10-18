import { describe, expect, it } from "vitest";

import {
  DEFAULT_PREFERENCES,
  effectiveTheme,
  motionReduced,
  resolvePreferences,
} from "./preferences";

describe("preferences", () => {
  it("falls back to defaults when nothing is set", () => {
    expect(resolvePreferences(() => undefined)).toEqual(DEFAULT_PREFERENCES);
  });

  it("reads and clamps values from settings", () => {
    const values: Record<string, unknown> = {
      "appearance.theme": "light",
      "appearance.ui-scale": 9,
      "editor.font-size": 3.4,
      "editor.line-height": 2,
      "editor.tab-size": 4,
      "editor.word-wrap": false,
      "editor.line-numbers": false,
      "preview.auto-compile": false,
      "preview.debounce-ms": 120,
      "preview.double-click-source": false,
      "preview.dark-pages": true,
    };
    const prefs = resolvePreferences((id) => values[id]);
    expect(prefs.theme).toBe("light");
    expect(prefs.uiScale).toBe(2);
    expect(prefs.editor).toEqual({
      fontSize: 8,
      lineHeight: 2,
      tabSize: 4,
      wordWrap: false,
      lineNumbers: false,
    });
    expect(prefs.preview).toEqual({
      autoCompile: false,
      debounceMs: 120,
      doubleClickSource: false,
      darkPages: true,
    });
  });

  it("defaults the PDF viewer like Overleaf: double-click goes to the source, pages light", () => {
    const prefs = resolvePreferences(() => undefined);
    expect(prefs.preview.doubleClickSource).toBe(true);
    expect(prefs.preview.darkPages).toBe(false);
    const odd = resolvePreferences((id) =>
      id === "preview.dark-pages"
        ? "yes"
        : id === "preview.double-click-source"
          ? 0
          : undefined,
    );
    expect(odd.preview.darkPages).toBe(false);
    expect(odd.preview.doubleClickSource).toBe(true);
  });

  it("ignores values of the wrong type", () => {
    const values: Record<string, unknown> = {
      "appearance.theme": "neon",
      "editor.font-size": "16",
      "editor.word-wrap": "yes",
    };
    const prefs = resolvePreferences((id) => values[id]);
    expect(prefs.theme).toBe("dark");
    expect(prefs.editor.fontSize).toBe(14);
    expect(prefs.editor.wordWrap).toBe(true);
  });

  it("resolves system theme and motion from the OS", () => {
    expect(effectiveTheme("system", true)).toBe("light");
    expect(effectiveTheme("system", false)).toBe("dark");
    expect(effectiveTheme("dark", true)).toBe("dark");
    expect(motionReduced("system", true)).toBe(true);
    expect(motionReduced("off", true)).toBe(false);
    expect(motionReduced("on", false)).toBe(true);
  });
});
