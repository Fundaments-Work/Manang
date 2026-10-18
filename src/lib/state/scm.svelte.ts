/**
 * Which of the Source Control view's two modes is showing: the developer's
 * Changes, or Versions (the timeline with Restore). One value so the View menu
 * and the panel's switch agree.
 */

import type { ScmMode } from "$lib/workbench/versions";

export class ScmState {
  mode = $state<ScmMode>("changes");

  show(mode: ScmMode): void {
    this.mode = mode;
  }
}

export const scm = new ScmState();
