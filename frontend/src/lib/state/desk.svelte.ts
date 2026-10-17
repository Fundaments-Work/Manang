/**
 * Desk view state.
 *
 * AGENTS.md: state lives close to the leaf components. This module holds only
 * what the Desk genuinely shares between its header and its panes — the three
 * view toggles from DESIGN.md §3.1. Anything engine-shaped (document text,
 * compiler diagnostics) belongs to the Rust core and reaches the shell as
 * events, not as store state.
 */

import { deck } from "./slides.svelte";

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

  /** Presentation Studio (DESIGN.md §8). */
  slideMode = $state(false);

  /** Fullscreen presentation, launched with F5. */
  presenting = $state(false);

  togglePane(): void {
    this.pane = this.pane === "split" ? "single" : "split";
  }

  toggleCanvas(): void {
    this.canvasOpen = !this.canvasOpen;
  }

  toggleZen(): void {
    this.zen = !this.zen;
  }

  /**
   * Zen mode is a superset: chrome stays hidden until it is dismissed.
   *
   * Slide and canvas overlays are suppressed in Zen because DESIGN.md §9 calls
   * for a zero-chrome reading surface, not a nested workspace.
   */
  get chromeHidden(): boolean {
    return this.zen;
  }

  /** True when an overlay owns the viewport and the Desk must stay behind it. */
  get overlayOpen(): boolean {
    return this.canvasOpen || this.shelfOpen || this.slideMode || this.presenting;
  }

  closeOverlays(): void {
    this.canvasOpen = false;
    this.shelfOpen = false;
    this.slideMode = false;
    this.presenting = false;
  }

  /**
   * Applies a location hash to the view state.
   *
   * Manang is local-first and has no server to route, but a document app still
   * needs to be able to say where it is: a slide deck shared as `#slides/3`,
   * a figure sent as `#draw`, a reading link as `#zen`. Hashes are read on load
   * and on `hashchange`, which also gives the screens a stable address for
   * end-to-end tests and screenshots.
   */
  applyDeepLink(hash: string): void {
    const route = hash.replace(/^#\/?/, "").trim();
    if (route.length === 0) return;

    const [head, argument] = route.split("/") as [string, string | undefined];

    switch (head) {
      case "library":
        this.libraryOpen = true;
        break;
      case "draw":
        this.canvasOpen = true;
        break;
      case "shelf":
        this.shelfOpen = true;
        break;
      case "zen":
        this.zen = true;
        break;
      case "split":
        this.pane = "single";
        break;
      case "slides": {
        this.slideMode = true;
        const index = Number.parseInt(argument ?? "", 10);
        if (Number.isFinite(index)) deck.goTo(index);
        break;
      }
      default:
        break;
    }
  }
}

export const desk = new DeskState();
