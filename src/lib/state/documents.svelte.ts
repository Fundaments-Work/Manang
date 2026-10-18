/**
 * Open text documents.
 *
 * Each open file is a `DocSession`: its CodeMirror state (text, undo history,
 * selection), the text as last saved, and a `KernelSync` that keeps a matching
 * document in the Rust editor plugin up to date. One `EditorView` shows
 * whichever session is active; `DocumentsStore.show` swaps states in and out.
 *
 * Dirty means "the text differs from what was last saved", decided by
 * structural equality of the CodeMirror documents, so undoing back to the saved
 * text clears the dot.
 */

import {
  Transaction,
  type ChangeSpec,
  type EditorState,
  type Text,
  type TransactionSpec,
} from "@codemirror/state";
import { EditorView, type ViewUpdate } from "@codemirror/view";
import { SvelteMap } from "svelte/reactivity";

import {
  applyKernelText,
  fromKernel,
  kernelSyncExtension,
  opsForTransactions,
} from "$lib/editor/kernelSync";
import {
  createEditorState,
  languageCompartment,
  liveCompartment,
  languageFor,
  loadLanguage,
  syncEditorPrefs,
  type EditorLanguage,
} from "$lib/editor/view";
import { kernel, errorMessage, reportError } from "$lib/kernel";
import { isConflictError, projectApi } from "$lib/kernel/api";
import type { ProjectChange } from "$lib/kernel/types";
import { decideExternal, isNotFoundError, type DiskNow } from "$lib/workbench/external";
import { basename } from "$lib/workbench/paths";
import { diffRange } from "$lib/kernel/offsets";
import { KernelSync, type SyncStatus } from "$lib/kernel/sync";
import type { LiveBinding, LiveHost } from "$lib/editor/live/binding";

import { notice } from "./notice.svelte";
import { dialog } from "./dialog.svelte";
import { workspace } from "./workspace.svelte";

export function countWords(text: string): number {
  let count = 0;
  let inWord = false;
  for (let i = 0; i < text.length; i += 1) {
    const space = /\s/.test(text.charAt(i));
    if (!space && !inWord) count += 1;
    inWord = !space;
  }
  return count;
}

export class DocSession {
  path: string;
  readonly language: EditorLanguage;
  /** Authoritative only while the session is not shown; see `doc`. */
  state: EditorState;
  savedDoc: Text;
  dirty = $state(false);
  /**
   * The file is out of step with this document: `changed` (it changed on disk
   * while there were unsaved edits) or `deleted` (it is gone or moved). The bar
   * above the editor shows it; nothing is overwritten until the person chooses.
   */
  external = $state<"changed" | "deleted" | null>(null);
  /** What is on disk now while `external` is `changed`: for Load and Compare. */
  diskNow: DiskNow | null = null;
  /** The Compare view of the bar is open. */
  comparing = $state(false);
  /**
   * Fingerprint of the file as last loaded or saved (`null`: the file does not
   * exist yet). The save path passes it so a change made outside is noticed.
   */
  diskHash: string | null;
  readonly sync: KernelSync;
  readonly #store: DocumentsStore;

  constructor(
    store: DocumentsStore,
    path: string,
    text: string,
    diskHash: string | null = null,
  ) {
    this.#store = store;
    this.path = path;
    this.diskHash = diskHash;
    this.language = languageFor(path);
    this.sync = new KernelSync(kernel, {
      applyText: (kernelText) => this.#applyKernelText(kernelText),
      onStatus: (status, detail) => store.syncStatus(this, status, detail),
    });
    this.state = createEditorState(text, [
      kernelSyncExtension(this.sync),
      EditorView.updateListener.of((update) => store.updated(this, update)),
    ]);
    this.savedDoc = this.state.doc;
    void this.sync.start(text);
    void loadLanguage(this.language)
      .then((support) =>
        this.#dispatch({ effects: languageCompartment.reconfigure(support) }),
      )
      .catch(() => {
        /* The document stays usable without syntax colour. */
      });
  }

  get #shown(): boolean {
    return this.#store.shown === this && this.#store.view !== undefined;
  }

  /** The live document: the view's while shown, the stored state's otherwise. */
  get doc(): Text {
    const view = this.#store.view;
    return this.#shown && view !== undefined ? view.state.doc : this.state.doc;
  }

  text(): string {
    return this.doc.toString();
  }

  #dispatch(spec: TransactionSpec): void {
    const view = this.#store.view;
    if (this.#shown && view !== undefined) view.dispatch(spec);
    else this.state = this.state.update(spec).state;
  }

  /** Apply the kernel's text without echoing it back, keeping caret and history. */
  #applyKernelText(text: string): void {
    const view = this.#store.view;
    if (this.#shown && view !== undefined) {
      applyKernelText(view, text);
      return;
    }
    const current = this.state.doc.toString();
    if (current === text) return;
    const { from, to, insert } = diffRange(current, text);
    this.state = this.state.update({
      changes: { from, to, insert },
      annotations: [
        fromKernel.of(true),
        Transaction.remote.of(true),
        Transaction.addToHistory.of(false),
      ],
    }).state;
  }

  /** Whether this document is in a live session (its edits go to the session). */
  live = $state(false);

  /** The document as the live binding reads and writes it. */
  liveHost(): LiveHost {
    return {
      state: () => {
        const view = this.#store.view;
        return this.#shown && view !== undefined ? view.state : this.state;
      },
      dispatch: (spec) => this.#dispatch(spec),
    };
  }

  /**
   * Join a live session: the binding takes over sending edits, so the kernel
   * copy (which the Rust session keeps) stops following the editor.
   */
  attachLive(binding: LiveBinding): void {
    this.sync.pause();
    this.live = true;
    this.#dispatch({ effects: liveCompartment.reconfigure(binding.extension) });
  }

  /** Leave the session: the kernel copy is brought up to what the editor shows. */
  detachLive(): Promise<void> {
    this.#dispatch({ effects: liveCompartment.reconfigure([]) });
    this.live = false;
    return this.sync.resume(this.text());
  }

  /** Caret and scroll, for the recovery snapshot and its restore. */
  viewState(): { anchor: number; head: number; scrollTop?: number } {
    const view = this.#store.view;
    const shown = this.#shown && view !== undefined;
    const main = (shown ? view.state : this.state).selection.main;
    return {
      anchor: main.anchor,
      head: main.head,
      ...(shown ? { scrollTop: view.scrollDOM.scrollTop } : {}),
    };
  }

  /**
   * Make the text `text` as an unsaved edit on top of what is on disk (a
   * recovered snapshot): the file is untouched, the tab turns dirty, and the
   * caret goes back where it was when the snapshot was taken.
   */
  applyUnsaved(text: string, caret?: { anchor: number; head: number }): void {
    const current = this.text();
    if (current !== text) {
      const { from, to, insert } = diffRange(current, text);
      const view = this.#store.view;
      const spec: TransactionSpec = { changes: { from, to, insert } };
      if (this.#shown && view !== undefined) view.dispatch(spec);
      else {
        const tr = this.state.update(spec);
        for (const batch of opsForTransactions([tr])) this.sync.submit(batch);
        this.state = tr.state;
      }
    }
    if (caret !== undefined) {
      const length = this.doc.length;
      const anchor = Math.min(Math.max(0, caret.anchor), length);
      const head = Math.min(Math.max(0, caret.head), length);
      this.#dispatch({ selection: { anchor, head } });
    }
    this.dirty = !this.doc.eq(this.savedDoc);
  }

  /**
   * Replace the text with what is on disk. It is an ordinary edit (not a kernel
   * echo), so the kernel's copy is brought along. It stays out of undo, except
   * when it throws away unsaved edits on purpose (`undoable`): Ctrl+Z then
   * brings them back.
   */
  loadFromDisk(
    text: string,
    hash: string | null = this.diskHash,
    undoable = false,
  ): void {
    this.diskHash = hash;
    const current = this.text();
    if (current !== text) {
      const { from, to, insert } = diffRange(current, text);
      const changes: ChangeSpec = { from, to, insert };
      const annotations = undoable ? [] : [Transaction.addToHistory.of(false)];
      const view = this.#store.view;
      if (this.#shown && view !== undefined) {
        view.dispatch({ changes, annotations });
      } else {
        const tr = this.state.update({ changes, annotations });
        for (const batch of opsForTransactions([tr])) this.sync.submit(batch);
        this.state = tr.state;
      }
    }
    this.savedDoc = this.doc;
    this.dirty = false;
    this.#inStep();
  }

  /** Record that `doc` is what is now on disk. */
  markSaved(doc: Text): void {
    this.savedDoc = doc;
    this.dirty = !this.doc.eq(doc);
    this.#inStep();
  }

  /**
   * The disk version becomes the baseline without touching the text: the
   * unsaved edits stay, and the next save replaces the file on purpose.
   * Also what "Keep my version" does.
   */
  rebase(content: string, hash: string | null): void {
    const view = this.#store.view;
    const shown = this.#shown && view !== undefined;
    this.savedDoc = (shown ? view.state : this.state).toText(content);
    this.diskHash = hash;
    this.dirty = !this.doc.eq(this.savedDoc);
    this.#inStep();
  }

  /** The file is gone: keep the text and expect a new file on the next save. */
  markDeleted(): void {
    this.external = "deleted";
    this.diskNow = null;
    this.comparing = false;
  }

  /** The disk changed under unsaved edits. */
  markChanged(disk: DiskNow): void {
    this.external = "changed";
    this.diskNow = disk;
  }

  #inStep(): void {
    this.external = null;
    this.diskNow = null;
    this.comparing = false;
  }
}

export class DocumentsStore {
  readonly #sessions = new SvelteMap<string, DocSession>();
  /** Bumped whenever a session is added, removed or renamed. */
  version = $state(0);
  view: EditorView | undefined;
  shown: DocSession | undefined;
  cursor = $state({ line: 1, column: 1 });
  words = $state(0);
  lines = $state(1);
  kernelOnline = $state(true);
  /** Bumped to ask the editor pane to take focus. */
  focusRequest = $state(0);
  revealRequest = $state.raw<{ path: string; line: number; column: number } | null>(
    null,
  );

  readonly #status = new SvelteMap<DocSession, SyncStatus>();
  readonly #editListeners: (() => void)[] = [];
  readonly #saveListeners: ((path: string) => void)[] = [];
  readonly #changeListeners: ((path: string) => void)[] = [];
  readonly #dropListeners: ((path: string) => void)[] = [];
  #statsTimer: ReturnType<typeof setTimeout> | undefined;

  /* ----------------------------------------------------------------------- */

  get(path: string): DocSession | undefined {
    return this.#sessions.get(path);
  }

  /** Every open document, for search and compile sources. */
  all(): DocSession[] {
    void this.version;
    return [...this.#sessions.values()];
  }

  get dirtyPaths(): string[] {
    return this.all()
      .filter((session) => session.dirty)
      .map((session) => session.path);
  }

  isDirty(path: string): boolean {
    return this.#sessions.get(path)?.dirty ?? false;
  }

  /** Called after any edit to any document; the build uses it to recompile. */
  onEdit(listener: () => void): void {
    this.#editListeners.push(listener);
  }

  /** Called with the path of the document each time its text changes. */
  onChange(listener: (path: string) => void): void {
    this.#changeListeners.push(listener);
  }

  /** Called with the path of a document that is being forgotten (tab closed, file gone). */
  onDrop(listener: (path: string) => void): void {
    this.#dropListeners.push(listener);
  }

  /** Called with the path after a document reached the disk. */
  onSave(listener: (path: string) => void): void {
    this.#saveListeners.push(listener);
  }

  /** Open a document's session, reading the file on first use. */
  async open(path: string): Promise<DocSession | null> {
    const existing = this.#sessions.get(path);
    if (existing !== undefined) return existing;
    try {
      const read = await workspace.readText(path);
      // Another call may have opened it while the read was in flight.
      const raced = this.#sessions.get(path);
      if (raced !== undefined) return raced;
      const session = new DocSession(this, path, read.content, read.hash);
      this.#sessions.set(path, session);
      this.version += 1;
      return session;
    } catch (error) {
      notice.error(`Could not open ${path}: ${errorMessage(error)}`);
      return null;
    }
  }

  /**
   * A document for a file that is not on disk (a new file recovered after a
   * crash): empty and clean, so applying the recovered text makes it dirty and
   * saving creates the file.
   */
  openMissing(path: string): DocSession {
    const existing = this.#sessions.get(path);
    if (existing !== undefined) return existing;
    const session = new DocSession(this, path, "", null);
    this.#sessions.set(path, session);
    this.version += 1;
    return session;
  }

  /** Forget a document (its tab closed or its file went away). */
  drop(path: string): void {
    const session = this.#sessions.get(path);
    if (session === undefined) return;
    for (const listener of this.#dropListeners) listener(path);
    this.#sessions.delete(path);
    this.#status.delete(session);
    if (this.shown === session) this.shown = undefined;
    this.version += 1;
    this.#refreshOnline();
  }

  dropUnder(path: string): void {
    for (const key of [...this.#sessions.keys()]) {
      if (key === path || key.startsWith(`${path}/`)) this.drop(key);
    }
  }

  renamed(from: string, to: string): void {
    for (const [key, session] of [...this.#sessions]) {
      if (key !== from && !key.startsWith(`${from}/`)) continue;
      const next = to + key.slice(from.length);
      this.#sessions.delete(key);
      session.path = next;
      this.#sessions.set(next, session);
    }
    this.version += 1;
  }

  /* ----------------------------------------------------------------------- */
  /* The shared editor view                                                   */
  /* ----------------------------------------------------------------------- */

  attach(view: EditorView): void {
    this.view = view;
  }

  /** Re-apply the editor preferences to the shown document. */
  refreshPrefs(): void {
    if (this.view !== undefined) syncEditorPrefs(this.view);
  }

  /** Release the view, keeping the shown session's state for next time. */
  detach(): void {
    if (this.view !== undefined && this.shown !== undefined) {
      this.shown.state = this.view.state;
    }
    this.shown = undefined;
    this.view = undefined;
  }

  /** Put `session` in the view. */
  show(session: DocSession): void {
    const view = this.view;
    if (view === undefined || this.shown === session) return;
    if (this.shown !== undefined) this.shown.state = view.state;
    this.shown = session;
    view.setState(session.state);
    syncEditorPrefs(view);
    this.#updateCursor(view.state);
    this.#refreshStats();
    if (session.sync.docId !== null) {
      void kernel
        .run("editor.activate", { id: session.sync.docId })
        .catch(() => undefined);
    }
  }

  focusEditor(): void {
    this.focusRequest += 1;
  }

  /** Move the caret to a 1-based line and column and scroll it into view. */
  revealAt(path: string, line: number, column: number): void {
    this.revealRequest = { path, line, column };
  }

  /** Apply a pending reveal if the right document is shown. */
  applyReveal(): void {
    const request = this.revealRequest;
    const view = this.view;
    if (request === null || view === undefined || this.shown?.path !== request.path)
      return;
    const doc = view.state.doc;
    const line = doc.line(Math.max(1, Math.min(request.line, doc.lines)));
    const at = Math.min(line.from + Math.max(0, request.column - 1), line.to);
    view.dispatch({
      selection: { anchor: at },
      effects: EditorView.scrollIntoView(at, { y: "center" }),
    });
    view.focus();
    this.revealRequest = null;
  }

  /** Insert text at the caret of the shown document. Returns false if none is shown. */
  insertAtCursor(text: string): boolean {
    const view = this.view;
    if (view === undefined || this.shown === undefined) return false;
    const range = view.state.selection.main;
    view.dispatch({
      changes: { from: range.from, to: range.to, insert: text },
      selection: { anchor: range.from + text.length },
    });
    view.focus();
    return true;
  }

  /* ----------------------------------------------------------------------- */
  /* Saving                                                                   */
  /* ----------------------------------------------------------------------- */

  async save(path: string): Promise<boolean> {
    const session = this.#sessions.get(path);
    if (session === undefined) return true;
    const doc = session.doc;
    try {
      const written = await workspace.writeFile(
        path,
        doc.toString(),
        session.diskHash ?? "absent",
      );
      return this.#saved(session, doc, written.hash);
    } catch (error) {
      if (isConflictError(error)) return this.#resolveConflict(session, doc);
      notice.error(`Could not save ${path}: ${errorMessage(error)}`);
      return false;
    }
  }

  #saved(session: DocSession, doc: Text, hash: string): true {
    session.diskHash = hash;
    session.markSaved(doc);
    for (const listener of this.#saveListeners) listener(session.path);
    return true;
  }

  /**
   * The file on disk is not the one this document loaded, so saving would
   * replace someone else's change. Ask; never decide silently.
   */
  async #resolveConflict(session: DocSession, doc: Text): Promise<boolean> {
    const path = session.path;
    let disk: DiskNow | null;
    try {
      disk = await workspace.readText(path);
    } catch (error) {
      if (!isNotFoundError(error)) {
        notice.error(`Could not save ${path}: ${errorMessage(error)}`);
        return false;
      }
      disk = null;
    }
    if (disk === null) {
      session.markDeleted();
      notice.error(
        `${basename(path)} was deleted or moved on disk. Use Save again or Save as in the bar above the editor.`,
      );
      return false;
    }
    const choice = await dialog.ask({
      title: `${basename(path)} changed on disk`,
      message:
        "Another program changed this file after Manang loaded it. Saving now would replace its changes with yours.",
      actions: [
        { id: "overwrite", label: "Overwrite with my version" },
        { id: "load", label: "Load disk version" },
        { id: "compare", label: "Compare" },
      ],
    });
    if (choice === "overwrite") {
      try {
        // The other program's version is kept in the project trash first.
        await projectApi(kernel).backup(path);
        const written = await workspace.writeFile(path, doc.toString(), disk.hash);
        return this.#saved(session, doc, written.hash);
      } catch (error) {
        notice.error(`Could not save ${path}: ${errorMessage(error)}`);
        return false;
      }
    }
    session.markChanged(disk);
    if (choice === "load") this.loadDisk(path);
    else if (choice === "compare") session.comparing = true;
    return false;
  }

  async saveAll(): Promise<void> {
    for (const path of this.dirtyPaths) await this.save(path);
  }

  /** Unsaved text keyed by path, for `typst.compile`'s `sources`. */
  dirtySources(): Record<string, string> {
    const sources: Record<string, string> = {};
    for (const session of this.all())
      if (session.dirty) sources[session.path] = session.text();
    return sources;
  }

  /** These files were written (a restore, say): see whether open documents follow. */
  externalChange(paths: readonly string[]): Promise<void> {
    return this.externalChanges(paths.map((path) => ({ path, kind: "modified" })));
  }

  /**
   * The watcher reported changes on disk. A clean open document reloads at once
   * (the caret stays where it was); one with unsaved edits is marked and the
   * bar asks; a file that went away keeps its tab. Never overwrites either side.
   */
  async externalChanges(changes: readonly ProjectChange[]): Promise<void> {
    for (const change of changes) {
      if (change.kind === "removed") {
        this.#vanished(change.path);
        continue;
      }
      if (change.kind === "renamed" && change.from !== undefined) {
        this.#vanished(change.from);
      }
      await this.syncWithDisk(change.path);
    }
  }

  /** A file or folder left its path: the documents under it lost their file. */
  #vanished(path: string): void {
    for (const [key, session] of this.#sessions) {
      if (key !== path && !key.startsWith(`${path}/`)) continue;
      if (session.external === "deleted") continue;
      session.markDeleted();
      notice.error(`${basename(key)} was deleted or moved on disk.`);
    }
  }

  /** Compare one open document with its file and act on the difference. */
  async syncWithDisk(path: string): Promise<void> {
    const session = this.#sessions.get(path);
    if (session === undefined) return;
    let disk: DiskNow | null;
    try {
      disk = await workspace.readText(path);
    } catch (error) {
      if (!isNotFoundError(error)) {
        reportError("documents.externalChange", error);
        return;
      }
      disk = null;
    }
    // The document may have been closed or typed into while the read ran.
    if (this.#sessions.get(path) !== session) return;
    const action = decideExternal(
      {
        dirty: session.dirty,
        diskHash: session.diskHash,
        savedText: session.savedDoc.toString(),
        text: session.dirty ? session.text() : "",
      },
      disk,
    );
    switch (action.kind) {
      case "ignore":
        if (disk !== null) {
          if (session.external === "deleted") session.rebase(disk.content, disk.hash);
          else session.diskHash = disk.hash;
        }
        break;
      case "reload":
        if (disk !== null) session.loadFromDisk(disk.content, disk.hash);
        break;
      case "adopt":
        if (disk !== null) session.rebase(disk.content, disk.hash);
        break;
      case "conflict":
        if (disk !== null) {
          const first = session.external === null;
          session.markChanged(disk);
          if (first) {
            notice.error(
              `${basename(path)} changed on disk. Your unsaved edits are kept: choose how to continue in the bar above the editor.`,
            );
          }
        }
        break;
      case "deleted":
        if (session.external !== "deleted") {
          session.markDeleted();
          notice.error(`${basename(path)} was deleted or moved on disk.`);
        }
        break;
    }
  }

  /** "Keep my version": the disk version is acknowledged and the next save replaces it. */
  keepMine(path: string): void {
    const session = this.#sessions.get(path);
    const disk = session?.diskNow;
    if (session === undefined || disk === null || disk === undefined) return;
    // The next save replaces the disk version: keep a copy of it now.
    projectApi(kernel)
      .backup(path)
      .catch((e: unknown) => reportError("documents.keepMine", e));
    session.rebase(disk.content, disk.hash);
  }

  /** "Load disk version": the unsaved edits are dropped (Ctrl+Z brings them back). */
  loadDisk(path: string): void {
    const session = this.#sessions.get(path);
    const disk = session?.diskNow;
    if (session === undefined || disk === null || disk === undefined) return;
    session.loadFromDisk(disk.content, disk.hash, true);
  }

  /** "Save again" on a deleted file: write it back where it was. */
  async recreate(path: string): Promise<boolean> {
    const session = this.#sessions.get(path);
    if (session === undefined) return false;
    session.diskHash = null;
    const saved = await this.save(path);
    if (!saved && session.external === null) session.markDeleted();
    return saved;
  }

  /* ----------------------------------------------------------------------- */
  /* Hooks from each session's editor state                                   */
  /* ----------------------------------------------------------------------- */

  updated(session: DocSession, update: ViewUpdate): void {
    if (update.docChanged) {
      session.dirty = !update.state.doc.eq(session.savedDoc);
      // Undone back to the saved text while the file moved on: follow the disk.
      if (!session.dirty && session.external === "changed")
        void this.syncWithDisk(session.path);
      for (const listener of this.#editListeners) listener();
      for (const listener of this.#changeListeners) listener(session.path);
      this.#scheduleStats();
    }
    if (this.shown === session && (update.docChanged || update.selectionSet)) {
      this.#updateCursor(update.state);
    }
  }

  syncStatus(session: DocSession, status: SyncStatus, detail?: string): void {
    this.#status.set(session, status);
    if (status === "offline" && detail !== undefined) notice.error(`Kernel: ${detail}`);
    this.#refreshOnline();
  }

  #refreshOnline(): void {
    this.kernelOnline = ![...this.#status.values()].includes("offline");
  }

  #updateCursor(state: EditorState): void {
    const head = state.selection.main.head;
    const line = state.doc.lineAt(head);
    this.cursor = { line: line.number, column: head - line.from + 1 };
  }

  /** Word count is O(n); do it once typing settles, not per keystroke. */
  #scheduleStats(): void {
    clearTimeout(this.#statsTimer);
    this.#statsTimer = setTimeout(() => this.#refreshStats(), 250);
  }

  #refreshStats(): void {
    const doc = this.shown?.doc;
    if (doc === undefined) return;
    this.words = countWords(doc.toString());
    this.lines = doc.lines;
  }
}

export const documents = new DocumentsStore();
