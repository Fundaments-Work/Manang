/**
 * A single small dialog: a confirmation (closing a document with unsaved
 * changes) or, with `input`, a one-field prompt whose actions are the choices
 * (naming a new diagram). `ask` resolves with the id of the chosen action, or
 * "cancel"; `prompt` also returns the field's text.
 */

export interface DialogAction {
  readonly id: string;
  readonly label: string;
}

export interface DialogInput {
  readonly label: string;
  readonly value: string;
}

export interface DialogRequest {
  readonly title: string;
  readonly message: string;
  readonly actions: readonly DialogAction[];
  /** A one-line text field shown above the actions. */
  readonly input?: DialogInput;
}

export class DialogState {
  request = $state.raw<DialogRequest | null>(null);
  /** The text field's current text, while a prompt is open. */
  value = $state("");
  #resolve: ((id: string) => void) | undefined;
  #answered = "";

  ask(request: DialogRequest): Promise<string> {
    // A second request while one is open cancels the first.
    this.#resolve?.("cancel");
    this.request = request;
    this.value = request.input?.value ?? "";
    return new Promise((resolve) => {
      this.#resolve = resolve;
    });
  }

  async prompt(request: DialogRequest): Promise<{ id: string; value: string }> {
    const id = await this.ask(request);
    return { id, value: this.#answered };
  }

  answer(id: string): void {
    const resolve = this.#resolve;
    this.#resolve = undefined;
    this.#answered = this.value;
    this.request = null;
    resolve?.(id);
  }
}

export const dialog = new DialogState();
