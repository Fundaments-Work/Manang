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
  languageFor,
  loadLanguage,
  syncEditorPrefs,
  type EditorLanguage,
} from "$lib/editor/view";
import { kernel, errorMessage } from "$lib/kernel";
import { diffRange } from "$lib/kernel/offsets";
import { KernelSync, type SyncStatus } from "$lib/kernel/sync";

import { notice } from "./notice.svelte";
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
  /** The file changed on disk while there were unsaved edits. */
  conflict = $state(false);
  readonly sync: KernelSync;
  readonly #store: DocumentsStore;

  constructor(store: DocumentsStore, path: string, text: string) {
    this.#store = store;
    this.path = path;
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

  /**
   * Replace the text with what is on disk. It is an ordinary edit (not a kernel
   * echo), so the kernel's copy is brought along, but it stays out of undo.
   */
  loadFromDisk(text: string): void {
    const current = this.text();
    if (current !== text) {
      const { from, to, insert } = diffRange(current, text);
      const changes: ChangeSpec = { from, to, insert };
      const view = this.#store.view;
      if (this.#shown && view !== undefined) {
        view.dispatch({ changes, annotations: Transaction.addToHistory.of(false) });
      } else {
        const tr = this.state.update({
          changes,
          annotations: Transaction.addToHistory.of(false),
        });
        for (const batch of opsForTransactions([tr])) this.sync.submit(batch);
        this.state = tr.state;
      }
    }
    this.savedDoc = this.doc;
    this.dirty = false;
    this.conflict = false;
  }

  /** Record that `doc` is what is now on disk. */
  markSaved(doc: Text): void {
    this.savedDoc = doc;
    this.dirty = !this.doc.eq(doc);
    this.conflict = false;
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

  /** Open a document's session, reading the file on first use. */
  async open(path: string): Promise<DocSession | null> {
    const existing = this.#sessions.get(path);
    if (existing !== undefined) return existing;
    try {
      const text = await workspace.readFile(path);
      // Another call may have opened it while the read was in flight.
      const raced = this.#sessions.get(path);
      if (raced !== undefined) return raced;
      const session = new DocSession(this, path, text);
      this.#sessions.set(path, session);
      this.version += 1;
      return session;
    } catch (error) {
      notice.error(`Could not open ${path}: ${errorMessage(error)}`);
      return null;
    }
  }

  /** Forget a document (its tab closed or its file went away). */
  drop(path: string): void {
    const session = this.#sessions.get(path);
    if (session === undefined) return;
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
      await workspace.writeFile(path, doc.toString());
      session.markSaved(doc);
      return true;
    } catch (error) {
      notice.error(`Could not save ${path}: ${errorMessage(error)}`);
      return false;
    }
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

  /** The kernel reported these files changed; reload the clean ones. */
  async externalChange(paths: readonly string[]): Promise<void> {
    for (const path of paths) {
      const session = this.#sessions.get(path);
      if (session === undefined) continue;
      let disk: string;
      try {
        disk = await workspace.readFile(path);
      } catch {
        continue;
      }
      if (disk === session.savedDoc.toString()) continue;
      if (session.dirty) {
        session.conflict = true;
        notice.error(
          `${path} changed on disk. Your unsaved edits are kept; saving will replace the disk version.`,
        );
      } else {
        session.loadFromDisk(disk);
      }
    }
  }

  /* ----------------------------------------------------------------------- */
  /* Hooks from each session's editor state                                   */
  /* ----------------------------------------------------------------------- */

  updated(session: DocSession, update: ViewUpdate): void {
    if (update.docChanged) {
      session.dirty = !update.state.doc.eq(session.savedDoc);
      for (const listener of this.#editListeners) listener();
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
