/**
 * State for the in-app Help viewer.
 *
 * Controls visibility, active documentation slug, and top-bar search query.
 */

import { getDoc } from "$lib/help/docs";

export class HelpPageState {
  open = $state(false);
  activeSlug = $state("index");
  query = $state("");

  show(slug?: string): void {
    this.open = true;
    if (slug !== undefined && getDoc(slug) !== undefined) {
      this.activeSlug = slug;
      this.query = "";
    }
  }

  hide(): void {
    this.open = false;
    this.query = "";
  }

  toggle(): void {
    if (this.open) {
      this.hide();
    } else {
      this.show();
    }
  }

  select(slug: string): void {
    if (getDoc(slug) !== undefined) {
      this.activeSlug = slug;
      this.query = "";
    }
  }
}

export const helpPage = new HelpPageState();
