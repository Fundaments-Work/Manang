/**
 * Read-only viewer tabs: images and PDFs from the project.
 *
 * Bytes come raw through `project.read-bytes` (never base64). An image is
 * shown through a `blob:` URL, revoked when its tab closes or the file is
 * reloaded; a PDF keeps its bytes for the PDF viewer.
 */

import { SvelteMap } from "svelte/reactivity";

import { errorMessage, kernel, reportError } from "$lib/kernel";
import { projectApi } from "$lib/kernel/api";
import { imageType, viewerKind, type ViewerKind } from "$lib/workbench/paths";

import { notice } from "./notice.svelte";

const api = projectApi(kernel);

export interface ViewedFile {
  readonly path: string;
  readonly kind: ViewerKind;
  /** Size on disk, in bytes. */
  readonly size: number;
  /** A `blob:` URL for an image; null for a PDF. */
  readonly url: string | null;
  /** The raw PDF; null for an image. */
  readonly pdf: Uint8Array | null;
}

/** Wrap bytes for display. Pure apart from `URL.createObjectURL`. */
export function viewedFile(path: string, bytes: Uint8Array): ViewedFile {
  const kind = viewerKind(path);
  if (kind === null) throw new Error(`${path} is not an image or a PDF.`);
  if (kind === "pdf") return { path, kind, size: bytes.length, url: null, pdf: bytes };
  const type = imageType(path) ?? "application/octet-stream";
  // Copy into a fresh ArrayBuffer: the IPC buffer may be a view on a larger one.
  const blob = new Blob([bytes.slice()], { type });
  return { path, kind, size: bytes.length, url: URL.createObjectURL(blob), pdf: null };
}

function release(file: ViewedFile): void {
  if (file.url !== null) URL.revokeObjectURL(file.url);
}

export class ViewersStore {
  readonly #open = new SvelteMap<string, ViewedFile>();

  get(path: string): ViewedFile | undefined {
    return this.#open.get(path);
  }

  async #load(path: string): Promise<ViewedFile> {
    return viewedFile(path, await api.readBytes(path));
  }

  /** Read the file and hold it for its tab. False (with a notice) on failure. */
  async open(path: string): Promise<boolean> {
    if (this.#open.has(path)) return true;
    try {
      this.#open.set(path, await this.#load(path));
      return true;
    } catch (error) {
      reportError("viewers.open", error);
      notice.error(`Could not open ${path}: ${errorMessage(error)}`);
      return false;
    }
  }

  /** Files changed on disk: reload the open ones so the tab shows what is there. */
  async externalChange(paths: readonly string[]): Promise<void> {
    for (const path of paths) {
      const old = this.#open.get(path);
      if (old === undefined) continue;
      try {
        const next = await this.#load(path);
        // The tab may have closed while reading.
        if (this.#open.get(path) !== old) {
          release(next);
          continue;
        }
        this.#open.set(path, next);
        release(old);
      } catch (error) {
        reportError("viewers.reload", error);
      }
    }
  }

  renamed(from: string, to: string): void {
    for (const [key, file] of [...this.#open]) {
      if (key !== from && !key.startsWith(`${from}/`)) continue;
      const next = to + key.slice(from.length);
      this.#open.delete(key);
      this.#open.set(next, { ...file, path: next });
    }
  }

  drop(path: string): void {
    const file = this.#open.get(path);
    if (file === undefined) return;
    this.#open.delete(path);
    release(file);
  }

  dropUnder(path: string): void {
    for (const key of [...this.#open.keys()]) {
      if (key === path || key.startsWith(`${path}/`)) this.drop(key);
    }
  }
}

export const viewers = new ViewersStore();
