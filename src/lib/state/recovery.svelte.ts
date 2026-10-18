/**
 * Crash recovery ("hot exit"): keeps a snapshot of every document with unsaved
 * edits in the recovery store (`recovery.*`, Rust), and offers them back after
 * a crash.
 *
 * This store only schedules and presents. The text is written, hashed, capped
 * and stored by the Rust plugin; nothing here reads or writes files. A snapshot
 * is written shortly after the last edit (`RecoveryScheduler`), removed when the
 * document is saved or its edits are discarded on purpose, and never overwrites
 * the file: recovered text opens as an unsaved edit.
 */

import { SvelteSet } from "svelte/reactivity";

import { serializeScene } from "./figure.svelte";
import { errorMessage, kernel, reportError } from "$lib/kernel";
import {
  projectApi,
  recoveryApi,
  type RecoveryEntry,
  type RecoverySnapshot,
  type TextDiff,
} from "$lib/kernel/api";
import { RecoveryScheduler } from "$lib/workbench/recoveryScheduler";
import { basename } from "$lib/workbench/paths";

import { documents } from "./documents.svelte";
import { figures } from "./figures.svelte";
import { notice } from "./notice.svelte";
import { tabs } from "./tabs.svelte";
import { workspace } from "./workspace.svelte";

const api = recoveryApi(kernel);
const project = projectApi(kernel);

/** One snapshot in the review dialog. */
export interface ReviewItem {
  readonly entry: RecoveryEntry;
  readonly snapshot: RecoverySnapshot;
  /** The file on disk against the recovered text; null for a figure. */
  readonly diff: TextDiff | null;
}

export class RecoveryStore {
  readonly #scheduler = new RecoveryScheduler((path) => void this.#flush(path));
  readonly #warned = new SvelteSet<string>();

  /** The review dialog's items and the one being shown, or null when closed. */
  review = $state.raw<{ items: readonly ReviewItem[]; index: number } | null>(null);

  /** Start following edits to documents and figures. Call once at boot. */
  start(): void {
    documents.onChange((path) => this.touch(path));
    documents.onSave((path) => this.forget(path));
    figures.onChange((path) => this.touch(path));
    figures.onSave((path) => this.forget(path));
  }

  /** An edit happened to `path`: a snapshot is due soon. */
  touch(path: string): void {
    if (workspace.project === null) return;
    this.#scheduler.touch(path);
  }

  /** The file was saved, or its edits were thrown away on purpose. */
  forget(path: string): void {
    this.#scheduler.cancel(path);
    const root = workspace.project?.root;
    if (root === undefined) return;
    this.#warned.delete(path);
    api.discard(root, path).catch((e: unknown) => reportError("recovery.discard", e));
  }

  /** Several files at once (closing with "Don't save"). */
  forgetAll(paths: readonly string[]): void {
    for (const path of paths) this.forget(path);
  }

  /** A file moved: its snapshot follows. */
  renamed(from: string, to: string): void {
    const root = workspace.project?.root;
    if (root === undefined) return;
    const due = this.#scheduler.isPending(from);
    this.#scheduler.cancel(from);
    api.discard(root, from).catch((e: unknown) => reportError("recovery.discard", e));
    if (due || documents.isDirty(to) || figures.isDirty(to)) this.touch(to);
  }

  /** Write every due snapshot now: before a dialog that may end in a close. */
  async flushAll(): Promise<void> {
    const keys = this.#scheduler.keys();
    this.#scheduler.dispose();
    await Promise.all(keys.map((path) => this.#flush(path)));
  }

  /** Drop pending writes without writing (the editors were reset). */
  cancelAll(): void {
    this.#scheduler.dispose();
  }

  async #flush(path: string): Promise<void> {
    const root = workspace.project?.root;
    if (root === undefined) return;
    try {
      const doc = documents.get(path);
      if (doc !== undefined) {
        if (!doc.dirty) {
          await api.discard(root, path);
          return;
        }
        const out = await api.write({
          project: root,
          path,
          kind: "text",
          content: doc.text(),
          savedHash: doc.diskHash,
          view: doc.viewState(),
        });
        this.#afterWrite(root, path, out.stored, out.limit, doc.dirty);
        return;
      }
      const figure = figures.get(path);
      if (figure !== undefined) {
        if (!figure.dirty) {
          await api.discard(root, path);
          return;
        }
        const out = await api.write({
          project: root,
          path,
          kind: "figure",
          content: serializeScene(figure.scene),
          savedHash: null,
        });
        this.#afterWrite(root, path, out.stored, out.limit, figure.dirty);
      }
    } catch (error) {
      reportError("recovery.write", error);
    }
  }

  /** A save that landed while the snapshot was in flight wins: drop the snapshot. */
  #afterWrite(
    root: string,
    path: string,
    stored: boolean,
    limit: number,
    stillDirty: boolean,
  ): void {
    if (!stored) {
      if (!this.#warned.has(path)) {
        this.#warned.add(path);
        notice.show(
          `${basename(path)} is over ${Math.round(limit / (1024 * 1024))} MB, so it cannot be protected against a crash. Save it often.`,
        );
      }
      return;
    }
    if (!stillDirty) {
      api.discard(root, path).catch((e: unknown) => reportError("recovery.discard", e));
    }
  }

  /* ----------------------------------------------------------------------- */
  /* After a crash                                                           */
  /* ----------------------------------------------------------------------- */

  /**
   * The snapshots of the open project worth offering: those that differ from
   * what is on disk. A snapshot identical to its file is removed quietly.
   */
  async pending(): Promise<RecoveryEntry[]> {
    const root = workspace.project?.root;
    if (root === undefined) return [];
    const listing = await api.list(root);
    const worth: RecoveryEntry[] = [];
    for (const entry of listing.entries) {
      if (documents.isDirty(entry.path) || figures.isDirty(entry.path)) continue;
      if (entry.kind === "text" && entry.disk !== "missing") {
        const [snap, disk] = await Promise.all([
          api.read(root, entry.path),
          project.readText(entry.path).catch(() => null),
        ]);
        if (snap !== null && disk !== null && snap.text === disk.content) {
          await api.discard(root, entry.path);
          continue;
        }
      }
      worth.push(entry);
    }
    return worth;
  }

  /** Open a snapshot as an unsaved edit. `open` opens the file's tab. */
  async restore(
    entry: RecoveryEntry,
    open: (path: string) => Promise<boolean>,
  ): Promise<boolean> {
    const root = workspace.project?.root;
    if (root === undefined) return false;
    try {
      const snap = await api.read(root, entry.path);
      if (snap === null) return false;
      if (entry.kind === "figure") return await this.#restoreFigure(snap, open);
      let session = documents.get(entry.path);
      if (session === undefined) {
        if (entry.disk === "missing") {
          session = documents.openMissing(entry.path);
          tabs.open(entry.path, undefined, "document");
        } else if (await open(entry.path)) {
          session = documents.get(entry.path);
        }
      }
      if (session === undefined) return false;
      const view = snap.view as { anchor?: unknown; head?: unknown } | undefined;
      const caret =
        typeof view?.anchor === "number" && typeof view.head === "number"
          ? { anchor: view.anchor, head: view.head }
          : undefined;
      session.applyUnsaved(snap.text, caret);
      // The edit is now the document's own: keep protecting it.
      this.touch(entry.path);
      return true;
    } catch (error) {
      reportError("recovery.restore", error);
      notice.error(`Could not recover ${entry.path}: ${errorMessage(error)}`);
      return false;
    }
  }

  async #restoreFigure(
    snap: RecoverySnapshot,
    open: (path: string) => Promise<boolean>,
  ): Promise<boolean> {
    const scene = JSON.parse(snap.text) as {
      width: number;
      height: number;
      background?: string;
      nodes: never[];
    };
    let figure = figures.get(snap.path);
    if (figure === undefined) {
      if (!(await open(snap.path))) {
        figure = await figures.create(snap.path);
      } else figure = figures.get(snap.path);
    }
    if (figure === undefined) return false;
    figure.edit({
      op: "batch",
      ops: [
        { op: "clear" },
        {
          op: "set-canvas",
          width: scene.width,
          height: scene.height,
          ...(scene.background !== undefined ? { background: scene.background } : {}),
        },
        { op: "add", nodes: scene.nodes },
      ],
    });
    return true;
  }

  /** Remove every snapshot of the open project (the user chose Discard). */
  async discardAll(): Promise<void> {
    const root = workspace.project?.root;
    if (root === undefined) return;
    await api.discard(root);
  }

  /* ----------------------------------------------------------------------- */
  /* Review                                                                  */
  /* ----------------------------------------------------------------------- */

  /** Load each snapshot with its diff against the disk and open the review. */
  async openReview(entries: readonly RecoveryEntry[]): Promise<void> {
    const root = workspace.project?.root;
    if (root === undefined) return;
    const items: ReviewItem[] = [];
    for (const entry of entries) {
      const snapshot = await api.read(root, entry.path);
      if (snapshot === null) continue;
      let diff: TextDiff | null = null;
      if (entry.kind === "text") {
        diff = await project.diffText(entry.path, snapshot.text).catch((e: unknown) => {
          reportError("recovery.diff", e);
          return null;
        });
      }
      items.push({ entry, snapshot, diff });
    }
    this.review = items.length === 0 ? null : { items, index: 0 };
  }

  closeReview(): void {
    this.review = null;
  }

  /** Move to the next item, or close when there is none. */
  nextReview(): void {
    const review = this.review;
    if (review === null) return;
    this.review =
      review.index + 1 >= review.items.length
        ? null
        : { items: review.items, index: review.index + 1 };
  }
}

export const recovery = new RecoveryStore();
