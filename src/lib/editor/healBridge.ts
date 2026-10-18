/**
 * The editor's door to self-healing.
 *
 * The callout and the quick-fix menu live in the editor layer and know nothing
 * about the kernel or the Problems model; the state layer
 * (`state/heal.svelte.ts`) registers the one function that applies a fix. The
 * same indirection keeps `editor/` free of `state/` imports.
 */

export interface FixRunner {
  /** Apply the fix `fixId` of the problem `problemId`. */
  apply(problemId: string, fixId: string): void;
}

let runner: FixRunner | null = null;

export function setFixRunner(next: FixRunner | null): void {
  runner = next;
}

/** Apply a fix; false when no runner is registered. */
export function runFix(problemId: string, fixId: string): boolean {
  if (runner === null) return false;
  runner.apply(problemId, fixId);
  return true;
}
