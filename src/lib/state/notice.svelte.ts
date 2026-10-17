/**
 * One quiet status message at a time, shown in the status bar and announced
 * politely to assistive technology. Confirmations clear themselves; errors stay
 * until the next message, because they say what to do.
 */

export type NoticeKind = "info" | "error";

const CLEAR_AFTER_MS = 5000;

export class NoticeState {
  text = $state("");
  kind = $state<NoticeKind>("info");
  #timer: ReturnType<typeof setTimeout> | undefined;

  show(text: string, kind: NoticeKind = "info"): void {
    clearTimeout(this.#timer);
    this.text = text;
    this.kind = kind;
    if (kind === "info") this.#timer = setTimeout(() => this.clear(), CLEAR_AFTER_MS);
  }

  error(text: string): void {
    this.show(text, "error");
  }

  clear(): void {
    clearTimeout(this.#timer);
    this.text = "";
  }
}

export const notice = new NoticeState();
