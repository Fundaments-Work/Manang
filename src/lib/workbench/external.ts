/**
 * What to do when a file that is open changes on disk (git pull, Syncthing,
 * another editor): a pure decision, so the rules are tested and the stores only
 * act on the answer.
 *
 * The rules, from "never lose either side":
 *  - the file is the one we already have (our own save, or the same bytes):
 *    nothing to do;
 *  - the document is clean: reload it silently;
 *  - the document has unsaved edits: keep both, show the bar, let the person
 *    choose; nothing is overwritten until they do;
 *  - the file is gone: keep the tab and the text, say so, offer Save or Save as.
 */

/** The file as found on disk now, or null when it no longer exists. */
export interface DiskNow {
  readonly content: string;
  readonly hash: string;
}

export interface OpenFileState {
  readonly dirty: boolean;
  /** Fingerprint of the file as last loaded or saved by this document. */
  readonly diskHash: string | null;
  /** The text this document last loaded or saved (what it believes is on disk). */
  readonly savedText: string;
  /** The text in the editor now. */
  readonly text: string;
}

export type ExternalAction =
  | { readonly kind: "ignore" }
  | { readonly kind: "reload" }
  /** Unsaved edits that now equal the file: the document is simply saved. */
  | { readonly kind: "adopt" }
  | { readonly kind: "conflict" }
  | { readonly kind: "deleted" };

export function decideExternal(
  file: OpenFileState,
  disk: DiskNow | null,
): ExternalAction {
  if (disk === null) return { kind: "deleted" };
  // Our own write coming back through the watcher, or a touch with no change.
  if (disk.hash === file.diskHash || disk.content === file.savedText) {
    return { kind: "ignore" };
  }
  if (!file.dirty) return { kind: "reload" };
  return disk.content === file.text ? { kind: "adopt" } : { kind: "conflict" };
}

/** The kernel's answer for a path that is not there (Rust and the browser mock). */
export function isNotFoundError(error: unknown): boolean {
  const message =
    typeof error === "object" && error !== null
      ? (error as { message?: unknown }).message
      : undefined;
  return (
    typeof message === "string" &&
    /does not exist|No such file|not found/i.test(message)
  );
}

/** The sentence for the bar above an editor whose file is out of step. */
export function externalMessage(
  name: string,
  state: "changed" | "deleted",
  dirty: boolean,
): string {
  if (state === "deleted") {
    return dirty
      ? `${name} was deleted or moved on disk. Your text is kept here.`
      : `${name} was deleted or moved on disk. The tab keeps its last text.`;
  }
  return `${name} changed on disk.`;
}
