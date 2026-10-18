/**
 * When to save a modified document by itself (the `files.autosave` setting).
 *
 * `after-delay` saves a file `delayMs` after the last edit to it; a save or a
 * close calls `cancel`. `on-focus-loss` saves every eligible document when the
 * window loses focus. Pure timing and rules: it knows nothing about documents,
 * so it is tested with fake timers.
 */

export type AutosaveMode = "off" | "after-delay" | "on-focus-loss";

export const AUTOSAVE_DELAY_MS = 1000;

export function autosaveMode(value: unknown): AutosaveMode {
  return value === "off" || value === "on-focus-loss" ? value : "after-delay";
}

/** What a document looks like to the rules below. */
export interface AutosaveCandidate {
  readonly dirty: boolean;
  /** The file is out of step with the disk: never saved over by itself. */
  readonly external: "changed" | "deleted" | null;
  /** The Compare view is open. */
  readonly comparing: boolean;
}

/** Autosave never overwrites a file that changed on disk or was removed. */
export function canAutosave(candidate: AutosaveCandidate | undefined): boolean {
  return (
    candidate !== undefined &&
    candidate.dirty &&
    candidate.external === null &&
    !candidate.comparing
  );
}

export class AutosaveScheduler {
  readonly #run: (path: string) => void;
  readonly #delay: number;
  readonly #timers = new Map<string, ReturnType<typeof setTimeout>>();

  constructor(run: (path: string) => void, delayMs: number = AUTOSAVE_DELAY_MS) {
    this.#run = run;
    this.#delay = delayMs;
  }

  /** An edit happened to `path`: save it once typing pauses. */
  touch(path: string): void {
    this.cancel(path);
    this.#timers.set(
      path,
      setTimeout(() => {
        this.#timers.delete(path);
        this.#run(path);
      }, this.#delay),
    );
  }

  cancel(path: string): void {
    const timer = this.#timers.get(path);
    if (timer === undefined) return;
    clearTimeout(timer);
    this.#timers.delete(path);
  }

  cancelAll(): void {
    for (const timer of this.#timers.values()) clearTimeout(timer);
    this.#timers.clear();
  }

  /** Paths with a save waiting. */
  get pending(): string[] {
    return [...this.#timers.keys()];
  }
}
