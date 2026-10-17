/**
 * Project thumbnails: page 1 of the main Typst document, stored through
 * `project.thumbnail-set` after a successful compile, at most once a minute
 * per project. Best effort: a failure here never reaches the user.
 */

import { kernel } from "$lib/kernel";
import { projectApi, typstApi } from "$lib/kernel/api";

/** Least time between two stored thumbnails of one project. */
export const THUMBNAIL_INTERVAL_MS = 60_000;
/** The kernel refuses larger ones; skip the round trip. */
const MAX_BYTES = 1024 * 1024;

const lastStored = new Map<string, number>();

/** Whether a thumbnail may be stored now. */
export function due(
  last: number | undefined,
  now: number,
  interval = THUMBNAIL_INTERVAL_MS,
): boolean {
  return last === undefined || now - last >= interval;
}

/** Forget what was stored (tests). */
export function resetThumbnailClock(): void {
  lastStored.clear();
}

/**
 * Store page 1 as the thumbnail of `root`. Call right after a successful
 * compile of the project's main file. `stored` runs after a thumbnail is saved
 * (Home uses it to reload the card). Resolves to whether one was stored.
 */
export async function captureThumbnail(
  root: string,
  now: number = Date.now(),
  stored?: (root: string) => void,
): Promise<boolean> {
  if (!due(lastStored.get(root), now)) return false;
  // Claim the slot first so overlapping compiles do not both render.
  lastStored.set(root, now);
  try {
    const bytes = await typstApi(kernel).render(0);
    if (bytes.byteLength === 0 || bytes.byteLength > MAX_BYTES) return false;
    await projectApi(kernel).setThumbnail(root, new TextDecoder().decode(bytes));
    stored?.(root);
    return true;
  } catch {
    // Let the next compile try again rather than waiting out the interval.
    lastStored.delete(root);
    return false;
  }
}
