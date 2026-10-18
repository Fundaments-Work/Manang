/**
 * The Settings page: a full page over the editor area (or over Home), opened
 * from the activity bar, Home, the command palette or a plugin's entry in the
 * Plugins view. `section` is a settings group id (`editor`, `latex`, ...) or
 * one of the page's other views (`keyboard`, `about`).
 */

export class SettingsPageState {
  open = $state(false);
  section = $state<string | null>(null);
  /** Typed into the top bar's search field while the page is open. */
  query = $state("");
  /** Bumped on every `show`, so the page can scroll to the section again. */
  request = $state(0);

  show(section: string | null = null): void {
    this.section = section;
    this.open = true;
    this.request += 1;
  }

  hide(): void {
    this.open = false;
    this.query = "";
  }

  toggle(): void {
    if (this.open) this.hide();
    else this.show();
  }
}

export const settingsPage = new SettingsPageState();
