/** Whether the Share popover (tab strip) is open. One piece of state, no logic. */
export class SharePopoverState {
  open = $state(false);
  /** Bumped on every `show`, so the popover can take focus again. */
  request = $state(0);

  show(): void {
    this.open = true;
    this.request += 1;
  }

  hide(): void {
    this.open = false;
  }

  toggle(): void {
    if (this.open) this.hide();
    else this.show();
  }
}

export const sharePopover = new SharePopoverState();
