/**
 * The recent custom colours of the figure studio: the last eight `#rrggbb`
 * values the colour picker applied, kept per user in `localStorage` (never in
 * a figure file). Storage that is missing or blocked just means the row does
 * not persist.
 */

import { loadRecent, pushRecent, saveRecent } from "$lib/figure/colour";

class RecentColours {
  list = $state.raw<readonly string[]>(loadRecent());

  /** Puts `hex` first; no-op for anything that is not a hex colour. */
  use(hex: string): void {
    const next = pushRecent(this.list, hex);
    this.list = next;
    saveRecent(next);
  }
}

export const recentColours = new RecentColours();
