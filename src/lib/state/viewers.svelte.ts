/**
 * Read-only viewer tabs: images, PDFs, text too large to edit, and an info
 * tab for every other file.
 *
 * Bytes come raw through `project.read-bytes` (never base64). An image is
 * shown through a `blob:` URL, revoked when its tab closes or the file is
 * reloaded; a PDF keeps its bytes for the PDF viewer; large text keeps only
 * the start of the file; a binary is only stat'ed (`project.stat`) for its
 * size and date, never read.
 */

import { SvelteMap } from "svelte/reactivity";

import { errorMessage, kernel, reportError } from "$lib/kernel";
import { projectApi } from "$lib/kernel/api";
import type { FileStat } from "$lib/kernel/types";
import { PREVIEW_BYTES, type OpenAs } from "$lib/workbench/opener";
import { imageType } from "$lib/workbench/paths";

import { notice } from "./notice.svelte";

const api = projectApi(kernel);

export type ViewerKind = "image" | "pdf" | "large-text" | "binary";

export interface ViewedFile {
  readonly path: string;
  readonly kind: ViewerKind;
  /** Size on disk, in bytes. */
  readonly size: number;
  /** Last modified (ms since the epoch), when known: the info tab shows it. */
  readonly modified: number | null;
  /** A `blob:` URL for an image. */
  readonly url: string | null;
  /** The raw PDF. */
  readonly pdf: Uint8Array | null;
  /** The start of a large text file. */
  readonly preview: string | null;
}

function isViewer(as: OpenAs): as is ViewerKind {
  return as === "image" || as === "pdf" || as === "large-text" || as === "binary";
}

/** Wrap bytes for display. Pure apart from `URL.createObjectURL`. */
export function viewedFile(
  path: string,
  kind: ViewerKind,
  bytes: Uint8Array,
): ViewedFile {
  const base = {
    path,
    kind,
    size: bytes.length,
    modified: null,
    url: null,
    pdf: null,
    preview: null,
  };
  switch (kind) {
    case "pdf":
      return { ...base, pdf: bytes };
    case "large-text":
      return {
        ...base,
        preview: new TextDecoder().decode(bytes.subarray(0, PREVIEW_BYTES)),
      };
    case "binary":
      return base;
    case "image": {
      const type = imageType(path) ?? "application/octet-stream";
      // Copy into a fresh buffer: the IPC bytes may be a view on a larger one.
      const blob = new Blob([bytes.slice()], { type });
      return { ...base, url: URL.createObjectURL(blob) };
    }
  }
}

/** The info tab of a file Manang cannot show: its size and date, never its bytes. */
export function binaryInfo(path: string, stat: FileStat): ViewedFile {
  return {
    path,
    kind: "binary",
    size: stat.size,
    modified: stat.modified,
    url: null,
    pdf: null,
    preview: null,
  };
}

/**
 * Load `path` for a viewer of `kind`: an info tab only stats the file
 * (`project.stat`); the others need the bytes (`project.read-bytes`).
 */
async function load(
  path: string,
  kind: ViewerKind,
  bytes?: Uint8Array,
): Promise<ViewedFile> {
  if (kind === "binary") return binaryInfo(path, await api.stat(path));
  return viewedFile(path, kind, bytes ?? (await api.readBytes(path)));
}

function release(file: ViewedFile): void {
  if (file.url !== null) URL.revokeObjectURL(file.url);
}

export class ViewersStore {
  readonly #open = new SvelteMap<string, ViewedFile>();

  get(path: string): ViewedFile | undefined {
    return this.#open.get(path);
  }

  /**
   * Hold `path` for its viewer tab, shown as `as`. `bytes` are the file's
   * contents when the caller already read them (to decide how to open it).
   * False, with a notice, on failure.
   */
  async open(path: string, as: OpenAs, bytes?: Uint8Array): Promise<boolean> {
    if (!isViewer(as)) return false;
    const existing = this.#open.get(path);
    if (existing !== undefined && existing.kind === as) return true;
    try {
      const next = await load(path, as, bytes);
      this.#open.set(path, next);
      if (existing !== undefined) release(existing);
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
        const next = await load(path, old.kind);
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
