/**
 * Desk view state.
 *
 * AGENTS.md: state lives close to the leaf components. This module holds only
 * what the Desk genuinely shares between its header and its panes — the three
 * view toggles from DESIGN.md §3.1. Anything engine-shaped (document text,
 * compiler diagnostics) belongs to the Rust core and reaches the shell as
 * events, not as store state.
 */

export type PaneMode = "single" | "split";

export class DeskState {
  /** Single-pane manuscript vs. manuscript + live paper sheet. */
  pane = $state<PaneMode>("split");

  /** The Canvas overlay (DESIGN.md §4). */
  canvasOpen = $state(false);

  /** Zen mode (DESIGN.md §9): dissolves all chrome. */
  zen = $state(false);

  /** Library rail (DESIGN.md §5). */
  libraryOpen = $state(false);

  /** Instrument shelf (DESIGN.md §6). */
  shelfOpen = $state(false);

  /** Command palette (DESIGN.md §7). */
  paletteOpen = $state(false);

  togglePane(): void {
    this.pane = this.pane === "split" ? "single" : "split";
  }

  toggleCanvas(): void {
    this.canvasOpen = !this.canvasOpen;
  }

  toggleZen(): void {
    this.zen = !this.zen;
  }

  /** Zen mode is a superset: chrome stays hidden until it is dismissed. */
  get chromeHidden(): boolean {
    return this.zen;
  }
}

export const desk = new DeskState();
