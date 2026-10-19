/**
 * Live co-editing session state.
 *
 * One session at a time, on one text document. The Rust `live` plugin owns the
 * CRDT, the encryption and the relay connection; this store starts and ends
 * the session, owns the `LiveBinding` that keeps the editor in step with it,
 * and holds what the session bar shows. The file on disk stays plain text:
 * Save writes what the editor shows, as always.
 */

import { LiveBinding, fromLive } from "$lib/editor/live/binding";
import { fromKernel } from "$lib/editor/kernelSync";
import { Transaction } from "@codemirror/state";
import { errorMessage, type KernelEvent } from "$lib/kernel/client";
import { kernel, reportError } from "$lib/kernel";
import { inTauri } from "$lib/kernel/log";
import {
  liveApi,
  liveError,
  type LiveConnection,
  type LivePeer,
  type LivePresenceEvent,
  type LiveRemoteEvent,
  type LiveStateEvent,
} from "$lib/kernel/liveApi";
import { basename } from "$lib/workbench/paths";
import { inviteFileLabel, NO_RELAY_MESSAGE } from "$lib/workbench/live";

import { documents, type DocSession } from "./documents.svelte";
import { dialog } from "./dialog.svelte";
import { notice } from "./notice.svelte";
import { settings } from "./settings.svelte";
import { settingsPage } from "./settingsPage.svelte";
import { tabs } from "./tabs.svelte";
import { workspace } from "./workspace.svelte";

const api = liveApi(kernel);

/** The settings the plugin needs, pushed in the browser where no shell does. */
const SYNCED = ["live.relay-url", "live.display-name"] as const;

type Early =
  | { topic: "live.remote"; payload: LiveRemoteEvent }
  | { topic: "live.presence"; payload: LivePresenceEvent }
  | { topic: "live.state"; payload: LiveStateEvent };

export class LiveState {
  /** The session's connection, or `off` when there is none. */
  connection = $state<LiveConnection | "off">("off");
  role = $state<"host" | "guest" | null>(null);
  /** The document in the session. */
  path = $state<string | null>(null);
  peers = $state.raw<readonly LivePeer[]>([]);
  attempt = $state(0);
  error = $state<string | null>(null);
  /** Shown when sharing needs a relay and none is set. */
  needsRelay = $state(false);
  /** Starting or joining. */
  busy = $state(false);
  /** The invite, shown for copying when the clipboard is not available. */
  inviteText = $state<string | null>(null);
  /** A copy just happened (the button says so for a moment). */
  copied = $state(false);

  #session: string | null = null;
  #binding: LiveBinding | null = null;
  #doc: DocSession | null = null;
  #listening = false;
  #early: Early[] = [];
  #copiedTimer: ReturnType<typeof setTimeout> | undefined;

  get active(): boolean {
    return this.connection !== "off";
  }

  /** Whether `path` is the document in the session. */
  isLive(path: string): boolean {
    return this.active && this.path === path;
  }

  /* ----------------------------------------------------------------------- */
  /* Events                                                                   */
  /* ----------------------------------------------------------------------- */

  listen(): void {
    if (this.#listening) return;
    this.#listening = true;
    documents.onDrop((path) => {
      if (this.path === path) void this.#abandon();
    });
    void kernel.onEvent((event) => this.#onEvent(event));
  }

  #onEvent(event: KernelEvent): void {
    if (!event.topic.startsWith("live.")) return;
    const payload = event.payload as { session?: unknown } | null;
    if (payload === null || typeof payload.session !== "string") return;
    const known = this.#session;
    if (known !== null && payload.session !== known) return;
    const item = { topic: event.topic, payload: event.payload } as Early;
    // Between the plugin's answer and the binding being attached, events are kept.
    if (this.#binding === null) {
      if (this.#early.length < 500) this.#early.push(item);
      return;
    }
    this.#deliver(item);
  }

  #deliver(event: Early): void {
    switch (event.topic) {
      case "live.remote":
        this.#binding?.remote(event.payload);
        break;
      case "live.presence":
        this.#binding?.presence(event.payload);
        break;
      case "live.state":
        this.#state(event.payload);
        break;
    }
  }

  #state(event: LiveStateEvent): void {
    this.connection = event.state;
    this.attempt = event.attempt;
    this.error = event.error;
    this.peers = event.peers;
    if (event.state === "error" && event.error !== null) notice.error(event.error);
  }

  /* ----------------------------------------------------------------------- */
  /* Settings and status                                                      */
  /* ----------------------------------------------------------------------- */

  /** Push the live settings to the plugin in the browser, where no shell does. */
  async #syncBrowser(): Promise<void> {
    if (inTauri()) return;
    await settings.load(workspace.project?.root ?? null);
    const values: Record<string, unknown> = {};
    for (const id of SYNCED) values[id] = settings.value(id);
    try {
      await api.configure(values);
    } catch (error) {
      reportError("The live settings were not accepted", error);
    }
  }

  /** At start-up: a session left running by a reloaded window has no editor; end it. */
  async reset(): Promise<void> {
    try {
      const status = await api.status();
      if (status.active) await api.leave();
    } catch {
      /* no live plugin (an older shell): nothing to end */
    }
  }

  /* ----------------------------------------------------------------------- */
  /* Share                                                                    */
  /* ----------------------------------------------------------------------- */

  /** The text document in front, if it can be shared. */
  #target(): DocSession | null {
    const tab = tabs.active;
    if (tab === undefined || tab.kind !== "document" || tab.view !== "text")
      return null;
    return documents.get(tab.path) ?? null;
  }

  /** Share the open file: start a session and put the invite on the clipboard. */
  async share(): Promise<void> {
    if (this.active || this.busy) return;
    const doc = this.#target();
    if (doc === null) {
      notice.error("Open a text file to share it live.");
      return;
    }
    this.listen();
    this.busy = true;
    this.needsRelay = false;
    try {
      await this.#syncBrowser();
      await doc.sync.idle();
      const docId = doc.sync.docId;
      if (docId === null) {
        notice.error("The document is not ready yet. Try again in a moment.");
        return;
      }
      const started = await api.start({ doc: docId, file: doc.path });
      this.#begin(doc, started.session, started.version, "host");
      this.inviteText = started.invite ?? null;
      await this.copyInvite();
    } catch (error) {
      const failure = liveError(error);
      if (failure.code === "no-relay") {
        this.needsRelay = true;
        notice.error(NO_RELAY_MESSAGE);
      } else {
        notice.error(failure.message);
      }
    } finally {
      this.busy = false;
    }
  }

  /* ----------------------------------------------------------------------- */
  /* Join                                                                     */
  /* ----------------------------------------------------------------------- */

  /** Ask for an invite and join the file it names (or the open one). */
  async join(invite?: string): Promise<void> {
    if (this.active || this.busy) return;
    let text = invite;
    if (text === undefined) {
      const answer = await dialog.prompt({
        title: "Join a live session",
        message:
          "Paste the invite you were sent. It starts with manang-live:// and carries the key that keeps the session private.",
        input: { label: "Invite", value: "" },
        actions: [{ id: "join", label: "Join", primary: true }],
      });
      if (answer.id !== "join" || answer.value.trim() === "") return;
      text = answer.value;
    }
    this.listen();
    this.busy = true;
    try {
      const doc = await this.#joinTarget(text);
      if (doc === null) return;
      await doc.sync.idle();
      const docId = doc.sync.docId;
      if (docId === null) {
        notice.error("The document is not ready yet. Try again in a moment.");
        return;
      }
      const name = basename(doc.path);
      let result;
      try {
        result = await api.join({ doc: docId, invite: text, name });
      } catch (error) {
        if (liveError(error).code !== "differs") throw error;
        const choice = await dialog.ask({
          title: `Replace your copy of ${name}?`,
          message:
            "Your copy of this file is different from the host's. Joining replaces it with the host's version. Your copy is saved first, outside the project, so nothing is lost.",
          actions: [{ id: "replace", label: "Use the host's version", primary: true }],
        });
        if (choice !== "replace") return;
        result = await api.join({
          doc: docId,
          invite: text,
          name,
          resolution: "take-host",
        });
      }
      this.#begin(doc, result.session, result.version, "guest");
      if (result.reset !== undefined && result.reset !== null) {
        const reset = result.reset;
        doc.liveHost().dispatch({
          changes: {
            from: reset.start,
            to: reset.start + reset.delete,
            insert: reset.insert,
          },
          annotations: [
            fromLive.of(true),
            fromKernel.of(true),
            Transaction.remote.of(true),
            Transaction.addToHistory.of(false),
          ],
        });
      }
      notice.show(`Joined ${inviteFileLabel(result.file)}.`);
    } catch (error) {
      notice.error(liveError(error).message);
    } finally {
      this.busy = false;
    }
  }

  /** The document an invite is for: the file it names if the project has it, else the open one. */
  async #joinTarget(invite: string): Promise<DocSession | null> {
    let file: string | null;
    try {
      file = (await api.inspect(invite)).file;
    } catch (error) {
      notice.error(liveError(error).message);
      return null;
    }
    if (file !== null) {
      const { openFile } = await import("./actions");
      if (await openFile(file, { focus: false })) {
        const named = documents.get(file);
        if (named !== undefined) return named;
      }
    }
    const open = this.#target();
    if (open === null) {
      notice.error("Open the file you want to join first, then paste the invite.");
    }
    return open;
  }

  /* ----------------------------------------------------------------------- */
  /* Begin and end                                                            */
  /* ----------------------------------------------------------------------- */

  #begin(
    doc: DocSession,
    session: string,
    version: number,
    role: "host" | "guest",
  ): void {
    this.#session = session;
    this.#doc = doc;
    this.role = role;
    this.path = doc.path;
    this.connection = "connecting";
    this.attempt = 0;
    this.error = null;
    this.peers = [];
    this.#binding = new LiveBinding({
      session,
      version,
      host: doc.liveHost(),
      api,
      onResync: (reason) =>
        notice.error(`The live document was re-read from the session: ${reason}.`),
    });
    doc.attachLive(this.#binding);
    for (const event of this.#early.splice(0)) {
      if ("session" in event.payload && event.payload.session === session) {
        this.#deliver(event);
      }
    }
  }

  /** Copy the invite; if the clipboard refuses, show it to copy by hand. */
  async copyInvite(): Promise<void> {
    if (!this.active) return;
    try {
      const { invite } = await api.invite();
      this.inviteText = invite;
      await navigator.clipboard.writeText(invite);
      this.inviteText = null;
      this.copied = true;
      clearTimeout(this.#copiedTimer);
      this.#copiedTimer = setTimeout(() => (this.copied = false), 2500);
      notice.show("Invite copied. Anyone who has it can edit this file with you.");
    } catch (error) {
      if (this.inviteText === null) reportError("live.copyInvite", error);
      // The bar shows the invite in a field that is selected for copying.
      notice.show("Select the invite in the bar and copy it.");
    }
  }

  /** Leave: edits on their way get a moment to arrive, then the session ends. */
  async leave(): Promise<void> {
    const binding = this.#binding;
    const doc = this.#doc;
    if (binding === null || doc === null) {
      this.#clear();
      return;
    }
    try {
      await binding.settle();
      await api.leave();
    } catch (error) {
      reportError("live.leave", error);
      notice.error(errorMessage(error));
    }
    binding.close();
    await doc.detachLive();
    this.#clear();
    notice.show("You left the live session. The file is still yours to save.");
  }

  /** The document went away (its tab closed): end the session without touching it. */
  async #abandon(): Promise<void> {
    const binding = this.#binding;
    try {
      await api.leave();
    } catch (error) {
      reportError("live.leave", error);
    }
    binding?.close();
    this.#clear();
  }

  #clear(): void {
    this.#session = null;
    this.#binding = null;
    this.#doc = null;
    this.#early = [];
    this.connection = "off";
    this.role = null;
    this.path = null;
    this.peers = [];
    this.attempt = 0;
    this.error = null;
    this.inviteText = null;
    this.copied = false;
  }

  /** Open Settings at the Collaboration page (the relay address is there). */
  openRelaySettings(): void {
    settingsPage.show("collaboration");
  }

  dismissRelayHint(): void {
    this.needsRelay = false;
  }
}

export const live = new LiveState();
