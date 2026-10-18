/**
 * When to write a recovery snapshot of an unsaved document.
 *
 * Every edit calls `touch(key)`. The snapshot is written `debounceMs` after the
 * last edit, but never later than `maxWaitMs` after the first edit that has not
 * been written yet, so a long burst of typing is still captured every few
 * seconds. A save or a discard calls `cancel(key)`. Pure timing: it knows
 * nothing about documents, so it is tested with fake timers.
 */

export const RECOVERY_DEBOUNCE_MS = 2000;
export const RECOVERY_MAX_WAIT_MS = 10_000;

export interface SchedulerOptions {
  readonly debounceMs?: number;
  readonly maxWaitMs?: number;
  /** Clock in milliseconds; defaults to `Date.now`. */
  readonly now?: () => number;
}

interface Pending {
  /** When the first not-yet-written edit happened. */
  readonly first: number;
  readonly timer: ReturnType<typeof setTimeout>;
}

export class RecoveryScheduler {
  readonly #flush: (key: string) => void;
  readonly #debounce: number;
  readonly #maxWait: number;
  readonly #now: () => number;
  readonly #pending = new Map<string, Pending>();

  constructor(flush: (key: string) => void, options: SchedulerOptions = {}) {
    this.#flush = flush;
    this.#debounce = options.debounceMs ?? RECOVERY_DEBOUNCE_MS;
    this.#maxWait = options.maxWaitMs ?? RECOVERY_MAX_WAIT_MS;
    this.#now = options.now ?? Date.now;
  }

  /** An edit happened: (re)schedule the snapshot of `key`. */
  touch(key: string): void {
    const now = this.#now();
    const existing = this.#pending.get(key);
    const first = existing?.first ?? now;
    if (existing !== undefined) clearTimeout(existing.timer);
    const delay = Math.max(0, Math.min(this.#debounce, first + this.#maxWait - now));
    const timer = setTimeout(() => {
      this.#pending.delete(key);
      this.#flush(key);
    }, delay);
    this.#pending.set(key, { first, timer });
  }

  /** The document was saved or its edits discarded: no snapshot is due. */
  cancel(key: string): void {
    const existing = this.#pending.get(key);
    if (existing === undefined) return;
    clearTimeout(existing.timer);
    this.#pending.delete(key);
  }

  /** A document moved: carry its due snapshot to the new key. */
  rename(from: string, to: string): void {
    if (!this.#pending.has(from)) return;
    this.cancel(from);
    this.touch(to);
  }

  /** Whether a snapshot of `key` is waiting to be written. */
  isPending(key: string): boolean {
    return this.#pending.has(key);
  }

  /** Every key with a snapshot due. */
  keys(): string[] {
    return [...this.#pending.keys()];
  }

  /** Write the due snapshot of `key` now (before a dialog or a close). */
  flushNow(key: string): void {
    if (!this.#pending.has(key)) return;
    this.cancel(key);
    this.#flush(key);
  }

  flushAll(): void {
    for (const key of this.keys()) this.flushNow(key);
  }

  /** Drop everything without writing. */
  dispose(): void {
    for (const key of this.keys()) this.cancel(key);
  }
}
