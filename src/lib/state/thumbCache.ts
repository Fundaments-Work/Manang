/**
 * Thumbnails as blob URLs, loaded on demand and kept in a small LRU.
 *
 * A card asks for its thumbnail only when it scrolls into view, the bytes come
 * over IPC as a raw `Uint8Array`, and at most `MAX_CACHED` object URLs live at
 * once; the oldest is revoked when a new one arrives.
 */

import { kernel } from "$lib/kernel";
import { projectApi } from "$lib/kernel/api";

const MAX_CACHED = 48;

/** key `${path}@${version}` to its object URL, oldest first. */
const cache = new Map<string, string>();
const inflight = new Map<string, Promise<string | null>>();

function remember(key: string, url: string): void {
  cache.set(key, url);
  while (cache.size > MAX_CACHED) {
    const oldest = cache.keys().next();
    if (oldest.done === true) break;
    const stale = cache.get(oldest.value);
    cache.delete(oldest.value);
    if (stale !== undefined) URL.revokeObjectURL(stale);
  }
}

export function loadThumbnail(path: string, version: number): Promise<string | null> {
  const key = `${path}@${version}`;
  const hit = cache.get(key);
  if (hit !== undefined) {
    // Refresh recency.
    cache.delete(key);
    cache.set(key, hit);
    return Promise.resolve(hit);
  }
  const pending = inflight.get(key);
  if (pending !== undefined) return pending;
  const task = projectApi(kernel)
    .thumbnail(path)
    .then((bytes) => {
      const url = URL.createObjectURL(
        new Blob([new Uint8Array(bytes)], { type: "image/svg+xml" }),
      );
      remember(key, url);
      return url;
    })
    .catch(() => null)
    .finally(() => inflight.delete(key));
  inflight.set(key, task);
  return task;
}
