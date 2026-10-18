/**
 * Applies the settings to the running shell: theme, interface scale, motion,
 * the editor's font, gutter and wrapping, and preview recompilation.
 *
 * Runs once from `boot()`. Every rule is in `workbench/preferences.ts`; this
 * file only watches the settings store and pushes the result to the DOM, the
 * editor and the build.
 */

import { setEditorPrefs } from "$lib/editor/view";
import { build } from "$lib/state/build.svelte";
import { documents } from "$lib/state/documents.svelte";
import { layout } from "$lib/state/layout.svelte";
import { settings } from "$lib/state/settings.svelte";
import {
  effectiveTheme,
  motionReduced,
  resolvePreferences,
  type Preferences,
} from "$lib/workbench/preferences";

function media(query: string): MediaQueryList | null {
  return typeof window !== "undefined" && typeof window.matchMedia === "function"
    ? window.matchMedia(query)
    : null;
}

let started = false;

/** Start applying settings. Safe to call more than once. */
export function applyPreferences(): void {
  if (started) return;
  started = true;

  const light = media("(prefers-color-scheme: light)");
  const reduced = media("(prefers-reduced-motion: reduce)");
  let systemLight = $state(light?.matches ?? false);
  let systemReduced = $state(reduced?.matches ?? false);
  light?.addEventListener("change", (e) => (systemLight = e.matches));
  reduced?.addEventListener("change", (e) => (systemReduced = e.matches));

  $effect.root(() => {
    const prefs: Preferences = $derived(resolvePreferences((id) => settings.value(id)));

    // The theme follows the setting once the settings are known; before that
    // the persisted layout theme avoids a flash at start-up.
    $effect(() => {
      if (!settings.loaded) return;
      const theme = effectiveTheme(prefs.theme, systemLight);
      if (layout.theme !== theme) layout.setTheme(theme);
    });

    $effect(() => {
      const root = document.documentElement;
      root.style.setProperty("--mng-editor-font-size", `${prefs.editor.fontSize}px`);
      root.style.setProperty(
        "--mng-editor-line-height",
        String(prefs.editor.lineHeight),
      );
      // `zoom` scales layout as well as paint, so hit targets grow with the text.
      root.style.zoom = prefs.uiScale === 1 ? "" : String(prefs.uiScale);
      root.classList.toggle(
        "reduce-motion",
        motionReduced(prefs.reducedMotion, systemReduced),
      );
    });

    $effect(() => {
      const { tabSize, wordWrap, lineNumbers } = prefs.editor;
      if (setEditorPrefs({ tabSize, wordWrap, lineNumbers })) documents.refreshPrefs();
    });

    $effect(() => {
      build.setPreviewPrefs(prefs.preview);
    });
  });
}
