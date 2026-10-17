/**
 * Runs a task once typing settles, never more often than `minGapMs`, and never
 * two at a time. Poking it again while it works schedules exactly one more run,
 * so a burst of edits costs one request, not one per keystroke.
 */

export const IDLE_MS = 400;
/** At most four requests per second per feature. */
export const MIN_GAP_MS = 250;

export class IdleRunner {
  #timer: ReturnType<typeof setTimeout> | undefined;
  #running = false;
  #again = false;
  #destroyed = false;
  #lastStart = Number.NEGATIVE_INFINITY;

  constructor(
    private readonly task: () => Promise<void>,
    private readonly idleMs = IDLE_MS,
    private readonly minGapMs: number | (() => number) = MIN_GAP_MS,
  ) {}

  poke(): void {
    if (this.#destroyed) return;
    clearTimeout(this.#timer);
    const gap = typeof this.minGapMs === "function" ? this.minGapMs() : this.minGapMs;
    const wait = Math.max(this.idleMs, this.#lastStart + gap - Date.now());
    this.#timer = setTimeout(() => void this.#fire(), wait);
  }

  async #fire(): Promise<void> {
    if (this.#destroyed) return;
    if (this.#running) {
      this.#again = true;
      return;
    }
    this.#running = true;
    this.#lastStart = Date.now();
    try {
      await this.task();
    } catch {
      /* A failed check leaves the previous marks; the next edit retries. */
    } finally {
      this.#running = false;
      if (this.#again) {
        this.#again = false;
        this.poke();
      }
    }
  }

  destroy(): void {
    this.#destroyed = true;
    clearTimeout(this.#timer);
  }
}
