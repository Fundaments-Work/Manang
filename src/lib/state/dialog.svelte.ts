/**
 * A single small dialog: a confirmation (closing a document with unsaved
 * changes) or, with `input`, a one-field prompt (naming a new diagram), which
 * may also offer one single-select `choice` (the diagram's kind). `ask`
 * resolves with the id of the chosen action, or "cancel"; `prompt` also
 * returns the field's text and the selected choice.
 */

import type { DialogAnswer, DialogRequest } from "$lib/workbench/dialogRequest";

export type {
  DialogAction,
  DialogAnswer,
  DialogChoice,
  DialogInput,
  DialogRequest,
} from "$lib/workbench/dialogRequest";

export class DialogState {
  request = $state.raw<DialogRequest | null>(null);
  /** The text field's current text, while a prompt is open. */
  value = $state("");
  /** The selected choice's id, while a prompt with a choice is open. */
  choice = $state("");
  #resolve: ((id: string) => void) | undefined;
  #answered = "";
  #chosen = "";

  ask(request: DialogRequest): Promise<string> {
    // A second request while one is open cancels the first.
    this.#resolve?.("cancel");
    this.request = request;
    this.value = request.input?.value ?? "";
    this.choice = request.choice?.value ?? "";
    return new Promise((resolve) => {
      this.#resolve = resolve;
    });
  }

  async prompt(request: DialogRequest): Promise<DialogAnswer> {
    const id = await this.ask(request);
    return { id, value: this.#answered, choice: this.#chosen };
  }

  answer(id: string): void {
    const resolve = this.#resolve;
    this.#resolve = undefined;
    this.#answered = this.value;
    this.#chosen = this.choice;
    this.request = null;
    resolve?.(id);
  }
}

export const dialog = new DialogState();
