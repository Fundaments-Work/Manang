/**
 * The Settings page: a full page over the editor area (or over Home), opened
 * from the activity bar, Home, the command palette or a plugin's entry in the
 * Plugins view. `section` is a settings group id (`editor`, `latex`, ...) or
 * one of the page's other views (`keyboard`, `about`).
 *
 * `origin` is where it was opened from: the page shows only that place's
 * settings (a Typst document: Editor, Preview, Compile, Typst, Writing,
 * Citations) and states the scope at the top, with one link to show all.
 * The workbench underneath is left as it was, so closing returns to the
 * same tab.
 */

import { APP_ORIGIN, type SettingsOrigin } from "$lib/workbench/settings";

export class SettingsPageState {
  open = $state(false);
  section = $state<string | null>(null);
  origin = $state.raw<SettingsOrigin>(APP_ORIGIN);
  /** Typed into the top bar's search field while the page is open. */
  query = $state("");
  /** Bumped on every `show`, so the page can scroll to the section again. */
  request = $state(0);

  /** Open at `section`, scoped to `origin` (everything when omitted). */
  show(section: string | null = null, origin: SettingsOrigin = APP_ORIGIN): void {
    this.section = section;
    this.origin = origin;
    this.open = true;
    this.request += 1;
  }

  /** "Show all settings": widen to the app scope without closing. */
  showAll(): void {
    this.origin = APP_ORIGIN;
  }

  hide(): void {
    this.open = false;
    this.query = "";
  }

  toggle(origin: SettingsOrigin = APP_ORIGIN): void {
    if (this.open) this.hide();
    else this.show(null, origin);
  }
}

export const settingsPage = new SettingsPageState();
