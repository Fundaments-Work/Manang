/**
 * Debounced, single-flight job scheduling.
 *
 * Requests inside the delay collapse into one run. While a run is in flight a
 * further request does not start a second one: it is remembered and runs once
 * afterwards. That is the right shape for compiles, which cannot be cancelled
 * in the engine but whose stale results must never replace newer ones.
 */

export interface Scheduler {
  /** Run after the delay; resets the timer if one is pending. */
  request(delay?: number): void;
  /** Run now (or right after the in-flight run), skipping the delay. */
  flush(): void;
  /** Drop a pending, not yet started run. An in-flight run is not affected. */
  cancel(): void;
  readonly pending: boolean;
  readonly running: boolean;
}

export function createScheduler(run: () => Promise<void>, delayMs: number): Scheduler {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let running = false;
  let again = false;

  async function start(): Promise<void> {
    timer = undefined;
    if (running) {
      again = true;
      return;
    }
    running = true;
    try {
      await run();
    } catch {
      // The job reports its own failures; the scheduler must keep working.
    } finally {
      running = false;
    }
    if (again) {
      again = false;
      void start();
    }
  }

  return {
    request(delay = delayMs) {
      clearTimeout(timer);
      timer = setTimeout(() => void start(), delay);
    },
    flush() {
      clearTimeout(timer);
      void start();
    },
    cancel() {
      clearTimeout(timer);
      timer = undefined;
      again = false;
    },
    get pending() {
      return timer !== undefined || again;
    },
    get running() {
      return running;
    },
  };
}
