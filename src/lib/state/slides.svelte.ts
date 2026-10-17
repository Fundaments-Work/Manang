/**
 * Slides view state. The deck itself is parsed from the document text
 * (`workbench/slides.ts`); this holds only which slide is current and whether
 * the deck is being presented full-screen.
 */

import { clampSlide } from "$lib/workbench/slides";

export class SlidesState {
  index = $state(0);
  presenting = $state(false);

  goTo(index: number, count: number): void {
    this.index = clampSlide(index, count);
  }
  next(count: number): void {
    this.goTo(this.index + 1, count);
  }
  previous(count: number): void {
    this.goTo(this.index - 1, count);
  }
  present(): void {
    this.presenting = true;
  }
  stop(): void {
    this.presenting = false;
  }
}

export const slides = new SlidesState();
