/**
 * The visual session of one editor view, and its handle.
 *
 * The session mirrors the CodeMirror document (ADR 0015 section 2): every
 * transaction, typed, remote or undone, is sent as `visual.edit`, batched to
 * one message per animation frame. After an edit the span table of the
 * viewport (plus a margin) is requested and swapped in; until it arrives the
 * old decorations stay, mapped through the changes, so nothing flickers.
 *
 * Failure never reaches the person. A malformed table, a lost session or a
 * failed call clears the table, and the document is plain source, which is
 * always correct.
 */

import type { ChangeSet} from "@codemirror/state";
import { type StateField } from "@codemirror/state";
import { ViewPlugin, type EditorView, type ViewUpdate } from "@codemirror/view";

import { parseVisualError } from "./client";
import { coverage, headingLevelAt, setTable, type VisualValue } from "./field";
import type { VisualHost, VisualStatus } from "./host";
import type { CommandState, EditPlan, Range } from "./protocol";
import type { SpanTable } from "./table";

/** Characters of margin requested around the viewport. */
export const MARGIN = 3000;

/** What the toolbar and the commands use of a running session. */
export interface VisualHandle {
  readonly session: number | null;
  readonly status: VisualStatus;
  /** Bumped by every document change: a plan made for another version is stale. */
  readonly version: number;
  /** Send pending edits and wait; true when the session matches the document. */
  settle(): Promise<boolean>;
  commandState(): Promise<CommandState[]>;
  headingLevel(): number;
  /** Call back after the document, selection or table changed. */
  subscribe(listener: () => void): () => void;
  /** Ask the host for an edit plan (`visual.run-command`) for the selection. */
  plan(command: string, args?: string): Promise<EditPlan | null>;
}

const activity = new Set<() => void>();
const handles = new WeakMap<EditorView, VisualHandle>();

/** The running visual session of a view, or null when it is in Source mode. */
export function visualHandle(view: EditorView | undefined): VisualHandle | null {
  return view === undefined ? null : (handles.get(view) ?? null);
}

/** Called whenever any visual session starts, changes or ends. */
export function onVisualActivity(listener: () => void): () => void {
  activity.add(listener);
  return () => activity.delete(listener);
}

/** The changes of a batch in the `ChangeSpec` form of the contract. */
export function specsOf(
  changes: ChangeSet,
): { from: number; to: number; insert: string }[] {
  const out: { from: number; to: number; insert: string }[] = [];
  changes.iterChanges((fromA, toA, _fromB, _toB, inserted) => {
    out.push({ from: fromA, to: toA, insert: inserted.toString() });
  });
  return out;
}

const intersects = (a: Range, b: Range): boolean => a.from <= b.to && a.to >= b.from;

export function createSync(
  host: VisualHost,
  field: StateField<VisualValue>,
): { plugin: ViewPlugin<Sync>; of(view: EditorView): VisualHandle | null } {
  const plugin = ViewPlugin.define((view) => new Sync(view, host, field));
  return { plugin, of: (view) => view.plugin(plugin) };
}

export class Sync implements VisualHandle {
  session: number | null = null;
  status: VisualStatus = "starting";
  version = 0;

  readonly #view: EditorView;
  readonly #host: VisualHost;
  readonly #field: StateField<VisualValue>;
  /** Local changes not yet sent, against the document the session holds. */
  #queue: ChangeSet | null = null;
  #chain: Promise<void> = Promise.resolve();
  #ackRevision = -1;
  #edits = 0;
  #dirty: Range | null = null;
  #frame = 0;
  #timer: ReturnType<typeof setTimeout> | undefined;
  #blocksScheduled = false;
  #token = 0;
  #failures = 0;
  #dead = false;
  readonly #listeners = new Set<() => void>();

  constructor(view: EditorView, host: VisualHost, field: StateField<VisualValue>) {
    this.#view = view;
    this.#host = host;
    this.#field = field;
    handles.set(view, this);
    this.#enqueue(() => this.#open());
    this.#changed();
  }

  /* ----------------------------------------------------------------------- */

  update(update: ViewUpdate): void {
    if (update.docChanged) {
      this.version += 1;
      this.#queue =
        this.#queue === null ? update.changes : this.#queue.compose(update.changes);
      this.#scheduleFlush();
    } else if (update.viewportChanged && !this.#covered()) {
      this.#scheduleBlocks();
    }
    if (update.docChanged || update.selectionSet || update.viewportChanged)
      this.#changed();
  }

  destroy(): void {
    this.#dead = true;
    if (handles.get(this.#view) === this) handles.delete(this.#view);
    cancelAnimationFrame(this.#frame);
    clearTimeout(this.#timer);
    this.#listeners.clear();
    const session = this.session;
    this.session = null;
    if (session !== null) void this.#host.api.close(session).catch(() => undefined);
    for (const listener of [...activity]) listener();
  }

  /* ----------------------------------------------------------------------- */
  /* Handle                                                                  */
  /* ----------------------------------------------------------------------- */

  settle(): Promise<boolean> {
    this.#enqueueFlush();
    return this.#chain.then(() => this.session !== null && this.#queue === null);
  }

  async commandState(): Promise<CommandState[]> {
    if (!(await this.settle()) || this.session === null) return [];
    const { from, to } = this.#view.state.selection.main;
    try {
      return await this.#host.api.commandState(this.session, from, to);
    } catch {
      return [];
    }
  }

  headingLevel(): number {
    const state = this.#view.state;
    return headingLevelAt(state, state.field(this.#field), state.selection.main.head);
  }

  async plan(command: string, args?: string): ReturnType<VisualHandle["plan"]> {
    for (let attempt = 0; attempt < 2; attempt += 1) {
      if (!(await this.settle()) || this.session === null) return null;
      const version = this.version;
      const { from, to } = this.#view.state.selection.main;
      let result;
      try {
        result = await this.#host.api.runCommand({
          session: this.session,
          command,
          from,
          to,
          ...(args === undefined ? {} : { args }),
        });
      } catch {
        return null;
      }
      // The document moved while the plan was made: it would land on other text.
      if (version === this.version) return result;
    }
    return null;
  }

  subscribe(listener: () => void): () => void {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  /* ----------------------------------------------------------------------- */
  /* Serial work                                                             */
  /* ----------------------------------------------------------------------- */

  #enqueue(task: () => Promise<void>): void {
    this.#chain = this.#chain
      .then(() => (this.#dead ? undefined : task()))
      .catch(() => undefined);
  }

  #changed(): void {
    for (const listener of [...this.#listeners]) listener();
    for (const listener of [...activity]) listener();
  }

  #setStatus(status: VisualStatus): void {
    if (this.status === status) return;
    this.status = status;
    this.#host.onStatus?.(status);
    this.#changed();
  }

  #scheduleFlush(): void {
    if (this.#frame !== 0) return;
    const run = (): void => {
      cancelAnimationFrame(this.#frame);
      clearTimeout(this.#timer);
      this.#frame = 0;
      this.#enqueueFlush();
    };
    this.#frame = requestAnimationFrame(run);
    // A hidden window does not draw frames; the edit must still go out.
    this.#timer = setTimeout(run, 50);
  }

  #enqueueFlush(): void {
    this.#enqueue(() => this.#flush());
  }

  #scheduleBlocks(): void {
    if (this.#blocksScheduled) return;
    this.#blocksScheduled = true;
    requestAnimationFrame(() => {
      this.#blocksScheduled = false;
      this.#enqueue(() => this.#requestBlocks());
    });
  }

  /* ----------------------------------------------------------------------- */

  async #open(): Promise<void> {
    const text = this.#snapshot();
    this.#setStatus("starting");
    try {
      const result = await this.#host.api.open({ text, path: this.#host.path });
      if (this.#dead) {
        void this.#host.api.close(result.session).catch(() => undefined);
        return;
      }
      this.session = result.session;
      this.#ackRevision = result.revision;
      this.#failures = 0;
      this.#setStatus("ready");
      await this.#requestBlocks();
    } catch (error) {
      // No adapter (or no kernel): the document stays plain source.
      const { code } = parseVisualError(error);
      this.#setStatus(code === "visual.no-adapter" ? "unavailable" : "degraded");
    }
  }

  /** The document text now; the session is built from it, so the queue is moot. */
  #snapshot(): string {
    this.#queue = null;
    return this.#view.state.doc.toString();
  }

  async #flush(): Promise<void> {
    if (this.#queue === null) return;
    if (this.session === null) {
      // Not open (a failed start): try again from the current text.
      await this.#open();
      return;
    }
    const queue = this.#queue;
    this.#queue = null;
    let dirty: Range | null;
    try {
      const result = await this.#host.api.edit({
        session: this.session,
        changes: specsOf(queue),
        len: queue.newLength,
      });
      this.#ackRevision = result.revision;
      dirty = result.dirty;
      this.#failures = 0;
    } catch (error) {
      await this.#recover(error);
      return;
    }
    this.#edits += 1;
    this.#dirty = dirty;
    if (this.#queue !== null) return; // more is coming; its batch will ask
    if (this.#edits > 1 || (this.#dirty !== null && this.#touchesWindow(this.#dirty))) {
      await this.#requestBlocks();
    }
  }

  /** A failed edit: rebuild the session from the document, once; else plain source. */
  async #recover(error: unknown): Promise<void> {
    const { code } = parseVisualError(error);
    this.#clearTable();
    this.#failures += 1;
    if (this.#failures > 3) {
      this.#setStatus("degraded");
      this.session = null;
      return;
    }
    const text = this.#snapshot();
    try {
      if (this.session !== null && code !== "visual.no-session") {
        const result = await this.#host.api.reset({ session: this.session, text });
        this.#ackRevision = result.revision;
      } else {
        const result = await this.#host.api.open({ text, path: this.#host.path });
        this.session = result.session;
        this.#ackRevision = result.revision;
      }
      this.#setStatus("ready");
      await this.#requestBlocks();
    } catch {
      this.session = null;
      this.#setStatus("degraded");
    }
  }

  #clearTable(): void {
    if (this.#dead || this.#view.state.field(this.#field).table === null) return;
    this.#view.dispatch({ effects: setTable.of(null) });
  }

  /* ----------------------------------------------------------------------- */
  /* Tables                                                                  */
  /* ----------------------------------------------------------------------- */

  #window(): Range {
    const { viewport, state } = this.#view;
    return {
      from: Math.max(0, viewport.from - MARGIN),
      to: Math.min(state.doc.length, viewport.to + MARGIN),
    };
  }

  #touchesWindow(range: Range): boolean {
    return intersects(range, this.#window());
  }

  /** Whether the current table covers what is on screen. */
  #covered(): boolean {
    const have = coverage(this.#view.state.field(this.#field));
    if (have === null) return false;
    const { viewport } = this.#view;
    return viewport.from >= have.from && viewport.to <= have.to;
  }

  async #requestBlocks(): Promise<void> {
    if (this.session === null || this.#dead) return;
    const token = (this.#token += 1);
    const { from, to } = this.#window();
    this.#edits = 0;
    this.#dirty = null;
    let table: SpanTable;
    try {
      const result = await this.#host.api.blocks({ session: this.session, from, to });
      if (!result.ok) {
        this.#clearTable();
        this.#setStatus("degraded");
        return;
      }
      table = result.table;
    } catch (error) {
      await this.#recover(error);
      return;
    }
    if (token !== this.#token) return;
    this.#accept(table, 0);
  }

  /**
   * Swap the table in if it describes the document the view has now: no local
   * change is waiting or in flight and the revisions match. Otherwise it is
   * dropped; the batch that made it stale will ask for a newer one.
   */
  #accept(table: SpanTable, tries: number): void {
    if (this.#dead) return;
    const fresh =
      this.#queue === null &&
      table.header.revision === this.#ackRevision &&
      table.header.docLen === this.#view.state.doc.length;
    if (!fresh) return;
    // Do not redraw under an IME composition: the browser owns that text.
    if (this.#view.composing && tries < 40) {
      setTimeout(() => this.#accept(table, tries + 1), 50);
      return;
    }
    this.#view.dispatch({ effects: setTable.of(table) });
    this.#setStatus("ready");
  }
}
