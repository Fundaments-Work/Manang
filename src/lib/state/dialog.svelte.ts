/**
 * A single confirmation dialog, used when closing a document with unsaved
 * changes. `ask` resolves with the id of the chosen action, or "cancel".
 */

export interface DialogAction {
  readonly id: string;
  readonly label: string;
}

export interface DialogRequest {
  readonly title: string;
  readonly message: string;
  readonly actions: readonly DialogAction[];
}

export class DialogState {
  request = $state.raw<DialogRequest | null>(null);
  #resolve: ((id: string) => void) | undefined;

  ask(request: DialogRequest): Promise<string> {
    // A second request while one is open cancels the first.
    this.#resolve?.("cancel");
    this.request = request;
    return new Promise((resolve) => {
      this.#resolve = resolve;
    });
  }

  answer(id: string): void {
    const resolve = this.#resolve;
    this.#resolve = undefined;
    this.request = null;
    resolve?.(id);
  }
}

export const dialog = new DialogState();
