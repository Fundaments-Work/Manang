/**
 * The web view's half of the live edit protocol.
 *
 * The Rust `live` plugin owns the CRDT and is the authority on order. The view
 * keeps the document the person types into, and a few milliseconds of
 * difference between the two is normal: a remote edit may be on its way while a
 * key is pressed. Both sides settle it with a version counter (the scheme of
 * CodeMirror's collab module, with Rust as the authority; see
 * `crates/manang-live-plugin/src/driver.rs`):
 *
 *  - `version` counts accepted steps: every remote batch and every accepted
 *    local batch is one step. The view knows the version it has integrated.
 *  - Local edits are pushed with that version. Rust applies them only if it is
 *    still at that version; otherwise it answers "stale" and the view first
 *    integrates the steps it has not seen, rebases its pending edits over them,
 *    and pushes again.
 *  - Remote batches and acknowledgements arrive on one ordered event stream, so
 *    each step is seen once and in order. Out-of-order arrivals are held.
 *
 * Pending edits are two change sets over the *confirmed* document (the state at
 * `version`): `inflight` (sent, not yet acknowledged) and `unsent` (typed since,
 * relative to confirmed + inflight). A remote batch R, relative to the
 * confirmed document, becomes `R.map(inflight).map(unsent)` for the view, and
 * the pending sets are mapped over it, so the screen and Rust meet at the same
 * text. Nothing here is a CRDT; there is no JS CRDT in the web view (ADR 0013).
 */

import {
  Annotation,
  Transaction,
  type ChangeSet,
  type EditorState,
  type Extension,
  type TransactionSpec,
} from "@codemirror/state";
import { EditorView } from "@codemirror/view";

import { fromKernel } from "$lib/editor/kernelSync";
import type {
  LiveApi,
  LiveChange,
  LivePresenceEvent,
  LiveRemoteEvent,
  LiveRemoteSelection,
} from "$lib/kernel/liveApi";
import { diffRange } from "$lib/kernel/offsets";

import {
  composeRemoteChanges,
  remoteSelectionsExtension,
  setRemoteSelections,
  type RemoteSelection,
} from "./remote";

/** Marks a transaction the binding itself made (remote steps, resyncs). */
export const fromLive = Annotation.define<boolean>();

/** Where the binding reads and writes the document it keeps in step. */
export interface LiveHost {
  state(): EditorState;
  dispatch(spec: TransactionSpec): void;
}

export interface BindingOptions {
  readonly session: string;
  /** The version the session was at when the binding attached. */
  readonly version: number;
  readonly host: LiveHost;
  readonly api: Pick<LiveApi, "push" | "selection" | "snapshot">;
  /** A step failed and the view was re-read from Rust (pending edits were lost). */
  readonly onResync?: (reason: string) => void;
  /** Wait this long for progress before re-reading the document. */
  readonly watchdogMs?: number;
  /** Caret announcements are batched over this long. */
  readonly selectionDelayMs?: number;
}

/** Sequential changes (last first), as Rust applies them, from a change set. */
export function toLiveChanges(changes: ChangeSet): LiveChange[] {
  const list: LiveChange[] = [];
  changes.iterChanges((fromA, toA, _fromB, _toB, inserted) => {
    list.push({ start: fromA, delete: toA - fromA, insert: inserted.toString() });
  });
  // Later changes first: applying them in this order keeps the earlier
  // positions valid, so Rust needs no remapping.
  return list.reverse();
}

export interface Rebased {
  /** The remote change as it applies to the view's document. */
  readonly view: ChangeSet;
  readonly inflight: ChangeSet | null;
  readonly unsent: ChangeSet | null;
}

/**
 * Integrate a remote batch (relative to the confirmed document) with the
 * pending edits. The remote step came first, so pending edits are mapped as
 * happening after it (`before = true`), and the remote change is mapped over
 * them as the earlier one (`before = false`). Both orders then give one text.
 */
export function rebase(
  remote: ChangeSet,
  inflight: ChangeSet | null,
  unsent: ChangeSet | null,
): Rebased {
  let r = remote;
  let nextInflight = inflight;
  let nextUnsent = unsent;
  if (inflight !== null) {
    nextInflight = inflight.map(r, true);
    r = r.map(inflight, false);
  }
  if (unsent !== null) {
    nextUnsent = unsent.map(r, true);
    r = r.map(unsent, false);
  }
  return { view: r, inflight: nextInflight, unsent: nextUnsent };
}

type SetTimer = (fn: () => void, ms: number) => ReturnType<typeof setTimeout>;

export class LiveBinding {
  readonly #session: string;
  readonly #host: LiveHost;
  readonly #api: BindingOptions["api"];
  readonly #onResync: (reason: string) => void;
  readonly #watchdogMs: number;
  readonly #selectionDelayMs: number;
  readonly #setTimer: SetTimer = (fn, ms) => setTimeout(fn, ms);

  #version: number;
  #inflight: ChangeSet | null = null;
  #unsent: ChangeSet | null = null;
  #sending = false;
  /** The push was accepted; its acknowledgement event has not arrived yet. */
  #awaitingAck = false;
  /** After a stale answer: push again once the view has reached this version. */
  #retryAt: number | null = null;
  readonly #held = new Map<number, LiveRemoteEvent>();
  #presence: LivePresenceEvent | null = null;
  #watchdog: ReturnType<typeof setTimeout> | undefined;
  #selectionTimer: ReturnType<typeof setTimeout> | undefined;
  #lastSelection = "";
  #resyncing: Promise<void> | null = null;
  #closed = false;
  /** How many times the view had to be re-read from Rust. */
  resyncs = 0;

  constructor(options: BindingOptions) {
    this.#session = options.session;
    this.#version = options.version;
    this.#host = options.host;
    this.#api = options.api;
    this.#onResync = options.onResync ?? (() => undefined);
    this.#watchdogMs = options.watchdogMs ?? 4000;
    this.#selectionDelayMs = options.selectionDelayMs ?? 80;
  }

  get session(): string {
    return this.#session;
  }

  /** The step the view has integrated. */
  get version(): number {
    return this.#version;
  }

  /** Whether edits are still on their way to Rust. */
  get pending(): boolean {
    return this.#inflight !== null || this.#unsent !== null;
  }

  /** Editor extensions: local edits and the caret go out, remote carets are drawn. */
  get extension(): Extension {
    return [
      EditorView.updateListener.of((update) => {
        if (this.#closed) return;
        for (const tr of update.transactions) {
          if (tr.docChanged && !isLiveTransaction(tr)) this.local(tr.changes);
        }
        if (update.docChanged || update.selectionSet) this.#queueSelection();
      }),
      remoteSelectionsExtension(),
    ];
  }

  /* ----------------------------------------------------------------------- */
  /* Local edits                                                              */
  /* ----------------------------------------------------------------------- */

  /** A local edit (relative to the view's document before it). */
  local(changes: ChangeSet): void {
    if (this.#closed) return;
    this.#unsent = this.#unsent === null ? changes : this.#unsent.compose(changes);
    this.#send();
  }

  #send(): void {
    if (this.#closed || this.#sending || this.#awaitingAck || this.#retryAt !== null)
      return;
    if (this.#inflight === null) {
      if (this.#unsent === null) return;
      this.#inflight = this.#unsent;
      this.#unsent = null;
    }
    this.#sending = true;
    this.#armWatchdog();
    const changes = toLiveChanges(this.#inflight);
    this.#api
      .push({ session: this.#session, version: this.#version, changes })
      .then((result) => {
        this.#sending = false;
        if (this.#closed) return;
        if (!result.accepted) {
          // Behind: the steps between are on their way (or already here).
          this.#retryAt = result.version;
          this.#afterProgress();
          return;
        }
        // Accepted: the acknowledgement event completes it (it may already
        // have). Edits typed while the push was in flight wait for that.
        if (result.version > this.#version) this.#awaitingAck = true;
        this.#send();
      })
      .catch((error: unknown) => {
        this.#sending = false;
        void this.resync(`a push failed (${describe(error)})`);
      });
  }

  /* ----------------------------------------------------------------------- */
  /* Steps from Rust                                                          */
  /* ----------------------------------------------------------------------- */

  /** A `live.remote` event for this session (others are ignored by the caller). */
  remote(event: LiveRemoteEvent): void {
    if (this.#closed || event.session !== this.#session) return;
    if (event.version <= this.#version) return; // already integrated
    this.#held.set(event.version, event);
    this.#drain();
  }

  /** A `live.presence` event: where the others' carets are. */
  presence(event: LivePresenceEvent): void {
    if (this.#closed || event.session !== this.#session) return;
    if (this.#presence !== null && event.version < this.#presence.version) return;
    this.#presence = event;
    this.#showPresence();
  }

  #drain(): void {
    for (;;) {
      const next = this.#held.get(this.#version + 1);
      if (next === undefined) break;
      this.#held.delete(next.version);
      try {
        this.#step(next);
      } catch (error) {
        void this.resync(`a step did not fit (${describe(error)})`);
        return;
      }
    }
    this.#afterProgress();
  }

  #step(event: LiveRemoteEvent): void {
    if (event.ack) {
      if (this.#inflight === null) {
        throw new Error("an acknowledgement arrived with nothing sent");
      }
      this.#inflight = null;
      this.#awaitingAck = false;
      this.#version = event.version;
      if (this.#unsent !== null) this.#send();
      return;
    }
    const confirmedLength = this.#confirmedLength();
    const remote = composeRemoteChanges(confirmedLength, event.changes);
    const { view, inflight, unsent } = rebase(remote, this.#inflight, this.#unsent);
    this.#inflight = inflight;
    this.#unsent = unsent;
    this.#version = event.version;
    this.#host.dispatch({
      changes: view,
      annotations: [
        fromLive.of(true),
        fromKernel.of(true),
        Transaction.remote.of(true),
        Transaction.addToHistory.of(false),
      ],
    });
  }

  /** Length of the document as Rust has it (without the pending edits). */
  #confirmedLength(): number {
    if (this.#inflight !== null) return this.#inflight.length;
    if (this.#unsent !== null) return this.#unsent.length;
    return this.#host.state().doc.length;
  }

  #afterProgress(): void {
    if (this.#retryAt !== null && this.#version >= this.#retryAt) {
      this.#retryAt = null;
      this.#send();
    }
    if (this.#held.size > 0 || this.#inflight !== null || this.#retryAt !== null) {
      this.#armWatchdog();
    } else {
      clearTimeout(this.#watchdog);
    }
    this.#showPresence();
  }

  /** No progress for a while while something is outstanding: re-read the document. */
  #armWatchdog(): void {
    clearTimeout(this.#watchdog);
    const before = this.#version;
    this.#watchdog = this.#setTimer(() => {
      if (this.#closed) return;
      const waiting =
        this.#held.size > 0 || this.#inflight !== null || this.#retryAt !== null;
      if (waiting && this.#version === before) {
        void this.resync("the session stopped answering");
      }
    }, this.#watchdogMs);
  }

  /* ----------------------------------------------------------------------- */
  /* Carets                                                                   */
  /* ----------------------------------------------------------------------- */

  #showPresence(): void {
    const event = this.#presence;
    if (event === null || event.version !== this.#version) return;
    // The carets are relative to the confirmed document; the view also holds
    // the pending edits.
    let pending: ChangeSet | null = this.#inflight;
    if (this.#unsent !== null) {
      pending = pending === null ? this.#unsent : pending.compose(this.#unsent);
    }
    const length = this.#host.state().doc.length;
    const selections: RemoteSelection[] = event.selections.map(
      (s: LiveRemoteSelection) => ({
        peer: s.peer,
        name: s.name,
        slot: s.slot,
        anchor: clamp(
          pending === null ? s.anchor : pending.mapPos(s.anchor, -1),
          length,
        ),
        head: clamp(pending === null ? s.head : pending.mapPos(s.head, -1), length),
      }),
    );
    this.#host.dispatch({
      effects: setRemoteSelections.of(selections),
      annotations: [
        fromLive.of(true),
        fromKernel.of(true),
        Transaction.addToHistory.of(false),
      ],
    });
  }

  #queueSelection(): void {
    clearTimeout(this.#selectionTimer);
    this.#selectionTimer = this.#setTimer(
      () => this.#sendSelection(),
      this.#selectionDelayMs,
    );
  }

  #sendSelection(): void {
    if (this.#closed) return;
    if (this.pending) {
      // Positions are only meaningful against the document Rust has.
      this.#queueSelection();
      return;
    }
    const main = this.#host.state().selection.main;
    const key = `${this.#version}:${main.anchor}:${main.head}`;
    if (key === this.#lastSelection) return;
    this.#lastSelection = key;
    void this.#api
      .selection({
        session: this.#session,
        version: this.#version,
        anchor: main.anchor,
        head: main.head,
      })
      .catch(() => undefined);
  }

  /* ----------------------------------------------------------------------- */
  /* Recovery and end                                                         */
  /* ----------------------------------------------------------------------- */

  /**
   * Replace the view's document with Rust's. The fallback when a step cannot be
   * applied or nothing answers; edits not yet acknowledged are lost, which is
   * why it is rare and reported.
   */
  resync(reason: string): Promise<void> {
    if (this.#resyncing !== null) return this.#resyncing;
    clearTimeout(this.#watchdog);
    const run = async (): Promise<void> => {
      try {
        const snapshot = await this.#api.snapshot(this.#session);
        if (this.#closed) return;
        const current = this.#host.state().doc.toString();
        if (current !== snapshot.text) {
          const { from, to, insert } = diffRange(current, snapshot.text);
          this.#host.dispatch({
            changes: { from, to, insert },
            annotations: [
              fromLive.of(true),
              fromKernel.of(true),
              Transaction.remote.of(true),
              Transaction.addToHistory.of(false),
            ],
          });
        }
        this.#version = snapshot.version;
        this.#inflight = null;
        this.#unsent = null;
        this.#retryAt = null;
        this.#awaitingAck = false;
        this.#held.clear();
        this.resyncs += 1;
        this.#onResync(reason);
      } catch (error) {
        this.#onResync(
          `${reason}; and the document could not be re-read (${describe(error)})`,
        );
      } finally {
        this.#resyncing = null;
      }
    };
    this.#resyncing = run();
    return this.#resyncing;
  }

  /**
   * Wait (briefly) for the edits on their way, then stop. Anything still
   * pending is left in the view and reconciled with the shared copy by the
   * caller.
   */
  async settle(limitMs = 1500): Promise<boolean> {
    const end = Date.now() + limitMs;
    while (this.pending && Date.now() < end) {
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
    return !this.pending;
  }

  /** Stop: no more pushes, no more steps applied. */
  close(): void {
    this.#closed = true;
    clearTimeout(this.#watchdog);
    clearTimeout(this.#selectionTimer);
    this.#held.clear();
  }
}

/** True for a transaction the binding made, which must not be sent back. */
export function isLiveTransaction(tr: Transaction): boolean {
  return tr.annotation(fromLive) === true;
}

function clamp(pos: number, length: number): number {
  return Math.max(0, Math.min(pos, length));
}

function describe(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}
