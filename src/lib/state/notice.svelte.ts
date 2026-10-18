/**
 * One quiet status message at a time, shown in the status bar and announced
 * politely to assistive technology. Confirmations clear themselves; errors stay
 * until the next message, because they say what to do. A message may carry one
 * action (Undo after a fix), which stays a little longer.
 */

export type NoticeKind = "info" | "error";

export interface NoticeAction {
  readonly label: string;
  readonly run: () => void;
}

const CLEAR_AFTER_MS = 5000;
const CLEAR_ACTION_AFTER_MS = 12000;

export class NoticeState {
  text = $state("");
  kind = $state<NoticeKind>("info");
  action = $state.raw<NoticeAction | null>(null);
  #timer: ReturnType<typeof setTimeout> | undefined;

  show(
    text: string,
    kind: NoticeKind = "info",
    action: NoticeAction | null = null,
  ): void {
    clearTimeout(this.#timer);
    this.text = text;
    this.kind = kind;
    this.action = action;
    if (kind === "info" || action !== null) {
      this.#timer = setTimeout(
        () => this.clear(),
        action === null ? CLEAR_AFTER_MS : CLEAR_ACTION_AFTER_MS,
      );
    }
  }

  error(text: string): void {
    this.show(text, "error");
  }

  /** Run the action once and drop the message. */
  runAction(): void {
    const action = this.action;
    this.clear();
    action?.run();
  }

  clear(): void {
    clearTimeout(this.#timer);
    this.text = "";
    this.action = null;
  }
}

export const notice = new NoticeState();
