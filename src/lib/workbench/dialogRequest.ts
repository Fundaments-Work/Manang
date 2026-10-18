/**
 * The shapes of the one small dialog (`state/dialog.svelte.ts`), and the
 * keyboard rule of its single-select: plain data, so the rules are tested.
 */

export interface DialogAction {
  readonly id: string;
  readonly label: string;
}

export interface DialogInput {
  readonly label: string;
  readonly value: string;
}

/** A compact single-select shown under the field (a segmented control). */
export interface DialogChoice {
  readonly label: string;
  readonly options: readonly DialogAction[];
  /** The id selected when the dialog opens. */
  readonly value: string;
}

export interface DialogRequest {
  readonly title: string;
  readonly message: string;
  /** The buttons after Cancel; the first is also what Enter in the field picks. */
  readonly actions: readonly DialogAction[];
  /** A one-line text field shown above the actions. */
  readonly input?: DialogInput;
  readonly choice?: DialogChoice;
}

export interface DialogAnswer {
  readonly id: string;
  readonly value: string;
  /** The selected choice's id, or "" when the request had no choice. */
  readonly choice: string;
}

/**
 * The option an arrow key moves to in a radiogroup: Left/Up go back, Right/Down
 * forward, Home/End to the ends, wrapping around. Null for any other key.
 */
export function stepChoice(
  options: readonly DialogAction[],
  current: string,
  key: string,
): string | null {
  if (options.length === 0) return null;
  const at = Math.max(
    0,
    options.findIndex((o) => o.id === current),
  );
  let next: number;
  switch (key) {
    case "ArrowLeft":
    case "ArrowUp":
      next = (at - 1 + options.length) % options.length;
      break;
    case "ArrowRight":
    case "ArrowDown":
      next = (at + 1) % options.length;
      break;
    case "Home":
      next = 0;
      break;
    case "End":
      next = options.length - 1;
      break;
    default:
      return null;
  }
  return options[next]?.id ?? null;
}
