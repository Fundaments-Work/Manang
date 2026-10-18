/**
 * Whether the keyboard shortcuts reference (Help > Keyboard shortcuts) is
 * showing. A store rather than component state so the `help.shortcuts`
 * command, the menu and the overlay's own Escape agree.
 */

class ShortcutOverlayState {
  open = $state(false);

  show(): void {
    this.open = true;
  }

  hide(): void {
    this.open = false;
  }

  toggle(): void {
    this.open = !this.open;
  }
}

export const shortcutOverlay = new ShortcutOverlayState();
