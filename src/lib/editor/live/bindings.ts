/**
 * The bindings of one live session, one per open file.
 *
 * A project session has many documents but the editor only holds the ones the
 * person opened. A binding is created when a text tab opens (`attach` asks the
 * session for that file's `{version, text}`) and disposed when it closes
 * (`detach`). Events for a file that has no binding yet are kept, per file, so
 * nothing between the plugin's answer and the binding is lost. A single-file
 * session is the same thing with one binding adopted at start.
 */

import { Transaction } from "@codemirror/state";

import { fromKernel } from "$lib/editor/kernelSync";
import type { LiveApi, LivePresenceEvent, LiveRemoteEvent } from "$lib/kernel/liveApi";
import { diffRange } from "$lib/kernel/offsets";

import { LiveBinding, fromLive, type LiveHost } from "./binding";

/** The part of a document session the bindings need (`DocSession` has it). */
export interface LiveDocument {
  readonly path: string;
  liveHost(): LiveHost;
  attachLive(binding: LiveBinding): void;
  detachLive(): Promise<void>;
  setLiveReadOnly(binding: LiveBinding, readOnly: boolean): void;
}

export type RoutedEvent =
  | { readonly topic: "live.remote"; readonly payload: LiveRemoteEvent }
  | { readonly topic: "live.presence"; readonly payload: LivePresenceEvent };

export interface BindingsOptions {
  readonly api: Pick<LiveApi, "push" | "selection" | "snapshot" | "attach" | "detach">;
  readonly onResync?: (path: string, reason: string) => void;
  readonly onLocalEdit?: (path: string) => void;
  /** `live.attach` was refused or timed out: the file stays a plain document. */
  readonly onAttachFailed?: (path: string, error: unknown) => void;
  /** Events kept per file while it has no binding. */
  readonly cap?: number;
  readonly watchdogMs?: number;
  readonly selectionDelayMs?: number;
}

/** Events with no `path` (a single-file plugin) are kept under this key. */
const UNNAMED = "";

export class LiveBindings {
  readonly #opts: BindingsOptions;
  readonly #cap: number;
  readonly #bindings = new Map<string, LiveBinding>();
  readonly #docs = new Map<string, LiveDocument>();
  readonly #early = new Map<string, RoutedEvent[]>();
  readonly #attaching = new Map<string, Promise<LiveBinding | null>>();
  #session: string | null = null;
  #readOnly = false;

  constructor(options: BindingsOptions) {
    this.#opts = options;
    this.#cap = options.cap ?? 500;
  }

  get session(): string | null {
    return this.#session;
  }

  /** Paths that have a binding. */
  paths(): string[] {
    return [...this.#bindings.keys()];
  }

  has(path: string): boolean {
    return this.#bindings.has(path);
  }

  get(path: string): LiveBinding | undefined {
    return this.#bindings.get(path);
  }

  /** How many events wait for `path` to get a binding. */
  buffered(path: string): number {
    return this.#early.get(path)?.length ?? 0;
  }

  /* ----------------------------------------------------------------------- */
  /* Events                                                                   */
  /* ----------------------------------------------------------------------- */

  /** Hand an event to its file's binding, or keep it until there is one. */
  route(event: RoutedEvent): void {
    const path = event.payload.path ?? UNNAMED;
    const binding =
      this.#bindings.get(path) ??
      (path === UNNAMED && this.#bindings.size === 1
        ? this.#bindings.values().next().value
        : undefined);
    if (binding !== undefined) {
      this.#deliver(binding, event);
      return;
    }
    const list = this.#early.get(path) ?? [];
    if (list.length < this.#cap) list.push(event);
    this.#early.set(path, list);
  }

  #deliver(binding: LiveBinding, event: RoutedEvent): void {
    if (event.topic === "live.remote") binding.remote(event.payload);
    else binding.presence(event.payload);
  }

  #flush(path: string, binding: LiveBinding, session: string): void {
    const keep = (e: RoutedEvent): boolean => e.payload.session === session;
    for (const key of [path, UNNAMED]) {
      if (key === UNNAMED && this.#bindings.size !== 1) continue;
      const list = this.#early.get(key);
      this.#early.delete(key);
      for (const event of list ?? []) if (keep(event)) this.#deliver(binding, event);
    }
  }

  /* ----------------------------------------------------------------------- */
  /* Bindings                                                                 */
  /* ----------------------------------------------------------------------- */

  /** A session begins: events kept for other sessions are dropped. */
  begin(session: string): void {
    this.#session = session;
    for (const [path, list] of this.#early) {
      const mine = list.filter((e) => e.payload.session === session);
      if (mine.length === 0) this.#early.delete(path);
      else this.#early.set(path, mine);
    }
  }

  #make(doc: LiveDocument, session: string, version: number): LiveBinding {
    const binding = new LiveBinding({
      session,
      path: doc.path,
      version,
      host: doc.liveHost(),
      api: this.#opts.api,
      onResync: (reason) => this.#opts.onResync?.(doc.path, reason),
      onLocalEdit: () => this.#opts.onLocalEdit?.(doc.path),
      ...(this.#opts.watchdogMs === undefined
        ? {}
        : { watchdogMs: this.#opts.watchdogMs }),
      ...(this.#opts.selectionDelayMs === undefined
        ? {}
        : { selectionDelayMs: this.#opts.selectionDelayMs }),
    });
    this.#bindings.set(doc.path, binding);
    this.#docs.set(doc.path, doc);
    doc.attachLive(binding);
    if (this.#readOnly) doc.setLiveReadOnly(binding, true);
    this.#flush(doc.path, binding, session);
    return binding;
  }

  /** Bind a document at the version the session was at (a single-file session). */
  adopt(doc: LiveDocument, session: string, version: number): LiveBinding {
    this.begin(session);
    return this.#make(doc, session, version);
  }

  /**
   * A text tab opened in a project session: ask the session for the file's
   * text, bring the view to it (without echoing), and bind. Concurrent calls
   * for one file share a result. `null` when the session ended meanwhile or
   * the file is not in it.
   */
  attach(doc: LiveDocument): Promise<LiveBinding | null> {
    const existing = this.#bindings.get(doc.path);
    if (existing !== undefined) return Promise.resolve(existing);
    const running = this.#attaching.get(doc.path);
    if (running !== undefined) return running;
    const session = this.#session;
    if (session === null) return Promise.resolve(null);
    const run = (async (): Promise<LiveBinding | null> => {
      try {
        const answer = await this.#opts.api.attach(doc.path);
        if (this.#session !== session) return null;
        const host = doc.liveHost();
        const current = host.state().doc.toString();
        if (current !== answer.text) {
          const { from, to, insert } = diffRange(current, answer.text);
          host.dispatch({
            changes: { from, to, insert },
            annotations: [
              fromLive.of(true),
              fromKernel.of(true),
              Transaction.remote.of(true),
              Transaction.addToHistory.of(false),
            ],
          });
        }
        return this.#make(doc, session, answer.version);
      } catch (error) {
        if (this.#session === session) this.#opts.onAttachFailed?.(doc.path, error);
        return null;
      } finally {
        this.#attaching.delete(doc.path);
      }
    })();
    this.#attaching.set(doc.path, run);
    return run;
  }

  /**
   * Stop keeping a file in step: its binding is closed and the document goes
   * back to a plain one. Tells the session unless `quiet` (it ended or the
   * file is gone).
   */
  async detach(path: string, options: { quiet?: boolean } = {}): Promise<void> {
    const binding = this.#bindings.get(path);
    const doc = this.#docs.get(path);
    this.#early.delete(path);
    if (binding === undefined) return;
    this.#bindings.delete(path);
    this.#docs.delete(path);
    binding.close();
    if (doc !== undefined) await doc.detachLive();
    if (options.quiet !== true) {
      await this.#opts.api.detach(path).catch(() => undefined);
    }
  }

  /** While a joiner waits for an answer no file can be typed into. */
  setReadOnly(readOnly: boolean): void {
    this.#readOnly = readOnly;
    for (const [path, binding] of this.#bindings) {
      this.#docs.get(path)?.setLiveReadOnly(binding, readOnly);
    }
  }

  /** Wait (briefly) for every file's edits on their way. */
  async settle(): Promise<void> {
    await Promise.all([...this.#bindings.values()].map((b) => b.settle()));
  }

  /** Close every binding and give the documents back (the session ended). */
  async closeAll(): Promise<void> {
    const docs = [...this.#docs.values()];
    for (const binding of this.#bindings.values()) binding.close();
    this.#bindings.clear();
    this.#docs.clear();
    this.#early.clear();
    this.#attaching.clear();
    this.#session = null;
    this.#readOnly = false;
    await Promise.all(docs.map((doc) => doc.detachLive()));
  }

  /** Close every binding without touching the documents (their tabs are gone). */
  abandon(): void {
    for (const binding of this.#bindings.values()) binding.close();
    this.#bindings.clear();
    this.#docs.clear();
    this.#early.clear();
    this.#attaching.clear();
    this.#session = null;
    this.#readOnly = false;
  }
}
