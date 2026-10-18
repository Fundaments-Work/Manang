/**
 * Editor <-> kernel synchronisation.
 *
 * The Rust editor plugin owns the document; CodeMirror holds a working copy.
 * User edits flow to Rust as minimal `editor.insert` / `editor.delete`
 * commands, one batch per CodeMirror transaction. Rust's revision counter
 * increments by exactly one per command, so the shell can predict the revision
 * after every command it sends. Any mismatch, failure or foreign change means
 * the copies may have diverged, and the answer is always the same: pull the
 * kernel's text and replace the local copy (`resync`).
 *
 * This module is framework-free. The view layer supplies `applyText` (apply
 * kernel text to CodeMirror, tagged so it is not echoed back) and calls
 * `submit` for each user transaction.
 */

import type { Kernel } from "./client";
import { diffRange, scalarLength, utf16ToScalar, type Sliceable } from "./offsets";

export type KernelOp =
  | { readonly kind: "delete"; readonly from: number; readonly to: number }
  | { readonly kind: "insert"; readonly at: number; readonly text: string };

/** What a CodeMirror `ChangeSet` offers; structural so tests need no editor. */
export interface ChangeIterable {
  iterChanges(
    f: (
      fromA: number,
      toA: number,
      fromB: number,
      toB: number,
      inserted: { toString(): string },
    ) => void,
  ): void;
}

/**
 * Convert one transaction's changes into kernel commands.
 *
 * `doc` is the document *before* the transaction and `changes` are expressed in
 * its coordinates. Offsets become Unicode scalar indices. Ops are emitted from
 * the last change to the first, so each op's index is still valid in the
 * kernel's document once the later ones have been applied: no remapping needed.
 */
export function toKernelOps(doc: Sliceable, changes: ChangeIterable): KernelOp[] {
  const perChange: KernelOp[][] = [];
  changes.iterChanges((fromA, toA, _fromB, _toB, inserted) => {
    const from = utf16ToScalar(doc, fromA);
    const removed = toA > fromA ? scalarLength(doc.sliceString(fromA, toA)) : 0;
    const text = inserted.toString();
    const group: KernelOp[] = [];
    // Within one change the delete runs first, then the insert at the same index.
    if (removed > 0) group.push({ kind: "delete", from, to: from + removed });
    if (text.length > 0) group.push({ kind: "insert", at: from, text });
    if (group.length > 0) perChange.push(group);
  });
  return perChange.reverse().flat();
}

export type SyncStatus = "idle" | "syncing" | "synced" | "offline";

interface DocInfo {
  readonly id: number;
  readonly chars: number;
  readonly revision: number;
}

export interface SyncOptions {
  /** Replace the editor's text with the kernel's. Must not echo back. */
  readonly applyText: (text: string) => void;
  readonly onStatus?: (status: SyncStatus, detail?: string) => void;
}

export class KernelSync {
  readonly #kernel: Kernel;
  readonly #options: SyncOptions;
  #tail: Promise<void> = Promise.resolve();
  #epoch = 0;
  #revision = 0;
  #docId: number | null = null;
  #status: SyncStatus = "idle";
  #resyncs = 0;
  #paused = false;

  constructor(kernel: Kernel, options: SyncOptions) {
    this.#kernel = kernel;
    this.#options = options;
  }

  get status(): SyncStatus {
    return this.#status;
  }

  /** Revision the shell believes the kernel is at. */
  get revision(): number {
    return this.#revision;
  }

  get docId(): number | null {
    return this.#docId;
  }

  /** How many times local state had to be replaced from the kernel. */
  get resyncCount(): number {
    return this.#resyncs;
  }

  /**
   * Whether the kernel's copy is being kept by something else (a live session:
   * the Rust side applies every accepted step to it). While paused nothing is
   * sent or compared; `resume` brings the copy up to what the editor shows.
   */
  get paused(): boolean {
    return this.#paused;
  }

  /** Stop sending edits; commands already queued still run. */
  pause(): void {
    this.#paused = true;
    this.#epoch += 1;
  }

  /**
   * Carry on: make the kernel's copy equal `text` (what the editor shows now)
   * with the smallest edit, then resume sending.
   */
  resume(text: string): Promise<void> {
    return this.#enqueue(async () => {
      if (this.#docId === null) {
        this.#paused = false;
        return;
      }
      try {
        const kernelText = await this.#kernel.run<string>("editor.text", {
          id: this.#docId,
        });
        if (kernelText !== text) {
          const { from, to, insert } = diffRange(kernelText, text);
          const at = utf16ToScalar(kernelText, from);
          const removed = scalarLength(kernelText.slice(from, to));
          if (removed > 0) {
            await this.#kernel.run("editor.delete", {
              id: this.#docId,
              from: at,
              to: at + removed,
            });
          }
          if (insert.length > 0) {
            await this.#kernel.run("editor.insert", {
              id: this.#docId,
              at,
              text: insert,
            });
          }
        }
        this.#revision = (await this.#info()).revision;
        this.#paused = false;
        this.#setStatus("synced");
      } catch (error) {
        this.#paused = false;
        this.#setStatus("offline", describe(error));
      }
    });
  }

  /** Resolves once every queued command has been sent. */
  idle(): Promise<void> {
    return this.#tail;
  }

  /** Create the kernel document holding `initialText`. */
  start(initialText: string): Promise<void> {
    return this.#enqueue(async () => {
      try {
        const created = await this.#kernel.run<{ id: number }>("editor.new");
        this.#docId = created.id;
        await this.#kernel.run("editor.activate", { id: created.id });
        if (initialText.length > 0) {
          await this.#kernel.run("editor.insert", {
            id: created.id,
            at: 0,
            text: initialText,
          });
        }
        const info = await this.#info();
        this.#revision = info.revision;
        this.#setStatus("synced");
      } catch (error) {
        this.#setStatus("offline", describe(error));
      }
    });
  }

  /** Queue the commands for one user transaction. Ordering is preserved. */
  submit(ops: readonly KernelOp[]): void {
    if (ops.length === 0 || this.#status === "offline" || this.#paused) return;
    const epoch = this.#epoch;
    void this.#enqueue(async () => {
      if (this.#docId === null) return;
      for (const op of ops) {
        if (epoch !== this.#epoch) return;
        try {
          const result =
            op.kind === "insert"
              ? await this.#kernel.run<{ revision: number }>("editor.insert", {
                  id: this.#docId,
                  at: op.at,
                  text: op.text,
                })
              : await this.#kernel.run<{ revision: number }>("editor.delete", {
                  id: this.#docId,
                  from: op.from,
                  to: op.to,
                });
          this.#revision += 1;
          if (result.revision !== this.#revision) {
            await this.#resync();
            return;
          }
        } catch (error) {
          await this.#resync(describe(error));
          return;
        }
      }
    });
  }

  /** Compare revisions and resync if the kernel changed behind our back. */
  check(): Promise<void> {
    return this.#enqueue(async () => {
      if (this.#docId === null || this.#paused) return;
      try {
        const info = await this.#info();
        if (info.revision !== this.#revision) await this.#resync();
      } catch (error) {
        this.#setStatus("offline", describe(error));
      }
    });
  }

  /** Unconditionally replace the editor's text with the kernel's. */
  resync(): Promise<void> {
    return this.#enqueue(() => (this.#paused ? Promise.resolve() : this.#resync()));
  }

  #enqueue(task: () => Promise<void>): Promise<void> {
    const next = this.#tail.then(task, task);
    // A task never rejects out of the queue; failures are folded into status.
    this.#tail = next.catch(() => undefined);
    return this.#tail;
  }

  async #info(): Promise<DocInfo> {
    return this.#kernel.run<DocInfo>("editor.info", { id: this.#docId });
  }

  /**
   * Pull text and revision from the kernel and apply them locally.
   *
   * The epoch is bumped before the first await and again after applying, so any
   * command submitted while the pull was in flight (computed against a document
   * that is about to be replaced) is discarded instead of being replayed onto
   * text it never saw.
   */
  async #resync(detail?: string): Promise<void> {
    if (this.#paused) return;
    this.#epoch += 1;
    this.#setStatus("syncing", detail);
    try {
      const [text, info] = await Promise.all([
        this.#kernel.run<string>("editor.text", { id: this.#docId }),
        this.#info(),
      ]);
      this.#options.applyText(text);
      this.#epoch += 1;
      this.#revision = info.revision;
      this.#resyncs += 1;
      this.#setStatus("synced");
    } catch (error) {
      this.#epoch += 1;
      this.#setStatus("offline", describe(error));
    }
  }

  #setStatus(status: SyncStatus, detail?: string): void {
    this.#status = status;
    this.#options.onStatus?.(status, detail);
  }
}

function describe(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}
