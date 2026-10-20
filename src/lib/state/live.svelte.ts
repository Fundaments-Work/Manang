/**
 * Live co-editing session state.
 *
 * One session at a time: of one text file (`scope: "file"`, the original) or of
 * a whole project (`scope: "project"`). The Rust `live` plugin owns the CRDTs,
 * the encryption and the relay connection; this store starts and ends the
 * session, owns the `LiveBindings` (one `LiveBinding` per open text file, made
 * when its tab opens and disposed when it closes) and holds what the session
 * UI shows: people and where they are, the join plan, progress, file notices
 * and follow mode. The files on disk stay plain text: Save writes what the
 * editor shows, as always.
 */

import { LiveBindings } from "$lib/editor/live/bindings";
import { fromLive } from "$lib/editor/live/binding";
import { fromKernel } from "$lib/editor/kernelSync";
import { Transaction } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { untrack } from "svelte";
import { SvelteMap, SvelteSet } from "svelte/reactivity";
import { errorMessage, type KernelEvent } from "$lib/kernel/client";
import { kernel, reportError } from "$lib/kernel";
import { inTauri } from "$lib/kernel/log";
import {
  liveApi,
  liveError,
  type LiveChoice,
  type LiveConnection,
  type LiveFileNode,
  type LiveNoticeEvent,
  type LivePeerAt,
  type LivePeersEvent,
  type LivePlanEvent,
  type LivePlanFile,
  type LiveProgress,
  type LiveProgressEvent,
  type LivePresenceEvent,
  type LiveRemoteEvent,
  type LiveScope,
  type LiveStateEvent,
  type LiveTreeEvent,
  type LiveTreeOp,
} from "$lib/kernel/liveApi";
import { basename } from "$lib/workbench/paths";
import { inviteFileLabel, NO_RELAY_MESSAGE } from "$lib/workbench/live";
import {
  followStep,
  NOT_FOLLOWING,
  type FollowEffect,
  type FollowEvent,
  type FollowState,
} from "$lib/workbench/liveFollow";
import {
  noticeLine,
  Throttled,
  treeSummary,
  type LiveNoticeLine,
} from "$lib/workbench/liveNotices";

import { documents, type DocSession } from "./documents.svelte";
import { dialog } from "./dialog.svelte";
import { notice } from "./notice.svelte";
import { settings } from "./settings.svelte";
import { settingsPage } from "./settingsPage.svelte";
import { tabs } from "./tabs.svelte";
import { workspace } from "./workspace.svelte";

const api = liveApi(kernel);

/** The settings the plugin needs, pushed in the browser where no shell does. */
const SYNCED = ["live.relay-url", "live.display-name", "live.email"] as const;

/** Events that carry no file: held until the session is known. */
type Early =
  | { topic: "live.state"; payload: LiveStateEvent }
  | { topic: "live.plan"; payload: LivePlanEvent }
  | { topic: "live.tree"; payload: LiveTreeEvent }
  | { topic: "live.peers"; payload: LivePeersEvent }
  | { topic: "live.progress"; payload: LiveProgressEvent }
  | { topic: "live.notice"; payload: LiveNoticeEvent };

const KEPT_OPS = 50;
const KEPT_NOTICES = 20;

export class LiveState {
  /** The session's connection, or `off` when there is none. */
  connection = $state<LiveConnection | "off">("off");
  role = $state<"host" | "guest" | null>(null);
  /** `file`: one document; `project`: the whole tree. `null` when off. */
  scope = $state<LiveScope | null>(null);
  /** The document of a file session (`null` for a project session). */
  path = $state<string | null>(null);
  /** The people in the session and the file each has in front of them. */
  peers = $state.raw<readonly LivePeerAt[]>([]);
  /** The files of a project session, with who has each open. */
  files = $state.raw<readonly LiveFileNode[]>([]);
  /** A joined project waits here for the guest to decide each file. */
  plan = $state.raw<readonly LivePlanFile[] | null>(null);
  /** Files being fetched after the plan was resolved. */
  progress = $state.raw<LiveProgress | null>(null);
  /** The person this window follows (peer id), if any. */
  following = $state<string | null>(null);
  /** Files deleted by someone else, and who: path -> name. */
  readonly deleted = new SvelteMap<string, string>();
  /** The latest tree changes (create, delete, rename), oldest first. */
  treeOps = $state.raw<readonly LiveTreeOp[]>([]);
  /** The latest people and file notices, oldest first. */
  notices = $state.raw<readonly LiveNoticeLine[]>([]);
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
  readonly #bindings = new LiveBindings({
    api,
    onResync: (path, reason) =>
      notice.error(`${basename(path)} was re-read from the session: ${reason}.`),
    onLocalEdit: () => this.#followEvent({ kind: "edited" }),
  });
  #listening = false;
  #early: Early[] = [];
  #copiedTimer: ReturnType<typeof setTimeout> | undefined;
  #follow: FollowState = NOT_FOLLOWING;
  /** Where each person's caret last was: peer -> file and offset. */
  readonly #heads = new SvelteMap<string, { path: string; head: number }>();
  readonly #inSession = new SvelteSet<string>();
  readonly #treeBatch = new Throttled<LiveTreeOp>((ops) => {
    this.treeOps = [...this.treeOps, ...ops].slice(-KEPT_OPS);
    const line = treeSummary(ops, (peer) => this.#nameOf(peer));
    if (line !== null) this.#addNotice(line, true);
  });
  readonly #peopleBatch = new Throttled<LiveNoticeLine>((lines) => {
    const [first] = lines;
    if (first === undefined) return;
    notice.show(
      lines.length === 1 ? first.text : `${lines.length} people joined or left.`,
    );
  });

  get active(): boolean {
    return this.connection !== "off";
  }

  /** Whether `path` is a file of the session (its edits go through it). */
  isLive(path: string): boolean {
    if (!this.active) return false;
    if (this.scope === "project") {
      return this.#inSession.has(path) && !this.deleted.has(path);
    }
    return this.path === path;
  }

  /** Who deleted `path` in the session, if someone did. */
  deletedBy(path: string): string | null {
    return this.deleted.get(path) ?? null;
  }

  /** People who have `path` in front of them. */
  peersAt(path: string): readonly LivePeerAt[] {
    return this.peers.filter((p) => p.path === path);
  }

  /* ----------------------------------------------------------------------- */
  /* Events                                                                   */
  /* ----------------------------------------------------------------------- */

  listen(): void {
    if (this.#listening) return;
    this.#listening = true;
    documents.onDrop((path) => {
      if (this.scope === "file" && this.path === path) void this.#abandon();
      else if (this.scope === "project" && this.#bindings.has(path)) {
        void this.#bindings.detach(path).catch(() => undefined);
      }
    });
    documents.onOpen((path) => {
      if (this.scope === "project" && this.isLive(path)) void this.#attach(path);
    });
    $effect.root(() => {
      $effect(() => {
        const path = tabs.active?.path ?? null;
        untrack(() => this.#followEvent({ kind: "navigated", path }));
      });
    });
    void kernel.onEvent((event) => this.#onEvent(event));
  }

  #onEvent(event: KernelEvent): void {
    if (!event.topic.startsWith("live.")) return;
    const payload = event.payload as { session?: unknown } | null;
    if (payload === null || typeof payload.session !== "string") return;
    const known = this.#session;
    if (known !== null && payload.session !== known) return;
    switch (event.topic) {
      case "live.remote":
        // Per file: the bindings keep what arrives before a file has one.
        this.#bindings.route({
          topic: "live.remote",
          payload: event.payload as LiveRemoteEvent,
        });
        return;
      case "live.presence": {
        const presence = event.payload as LivePresenceEvent;
        this.#bindings.route({ topic: "live.presence", payload: presence });
        this.#heard(presence);
        return;
      }
    }
    const item = { topic: event.topic, payload: event.payload } as Early;
    if (known === null) {
      if (this.#early.length < 500) this.#early.push(item);
      return;
    }
    this.#deliver(item);
  }

  #deliver(event: Early): void {
    switch (event.topic) {
      case "live.state":
        this.#state(event.payload);
        break;
      case "live.plan":
        this.plan = event.payload.files;
        break;
      case "live.tree":
        this.#tree(event.payload.ops);
        break;
      case "live.peers":
        this.#setPeers(event.payload.peers);
        break;
      case "live.progress": {
        const { done, total } = event.payload;
        this.progress = done >= total ? null : { done, total };
        if (done >= total) void this.#afterFiles();
        break;
      }
      case "live.notice":
        this.#noticed(event.payload);
        break;
    }
  }

  #state(event: LiveStateEvent): void {
    this.connection = event.state;
    this.attempt = event.attempt;
    this.error = event.error;
    this.#setPeers(event.peers);
    this.#bindings.setReadOnly(event.state === "waiting");
    if (event.state === "error" && event.error !== null) notice.error(event.error);
  }

  #setPeers(incoming: readonly LivePeerAt[]): void {
    const before: Record<string, string | null> = {};
    for (const p of this.peers) before[p.peer] = p.path ?? null;
    this.peers = incoming.map((p) => ({
      ...p,
      path: p.path === undefined ? (before[p.peer] ?? null) : p.path,
    }));
    const followed = this.following;
    if (followed === null) return;
    const now = this.peers.find((p) => p.peer === followed);
    if (now === undefined) {
      this.#followEvent({ kind: "peer-gone", peer: followed });
      return;
    }
    const head = this.#heads.get(followed);
    this.#followEvent({
      kind: "peer-at",
      peer: followed,
      path: now.path ?? null,
      head: head !== undefined && head.path === now.path ? head.head : null,
    });
  }

  /** Where the people's carets are, for follow mode. */
  #heard(event: LivePresenceEvent): void {
    const path = event.path ?? this.path;
    if (path === null) return;
    for (const s of event.selections) {
      this.#heads.set(s.peer, { path, head: s.head });
      if (this.following === s.peer) {
        this.#followEvent({ kind: "peer-at", peer: s.peer, path, head: s.head });
      }
    }
  }

  #nameOf(peer: string): string {
    return this.peers.find((p) => p.peer === peer)?.name ?? "Someone";
  }

  #addNotice(line: LiveNoticeLine, toast: boolean): void {
    this.notices = [...this.notices, line].slice(-KEPT_NOTICES);
    if (toast) notice.show(line.text);
  }

  #noticed(event: LiveNoticeEvent): void {
    const line = noticeLine(event, (peer) => this.#nameOf(peer));
    if (event.kind === "joined" || event.kind === "left") {
      this.#addNotice(line, false);
      this.#peopleBatch.push(line);
    } else {
      // Files: the tree event raises the toast; this only keeps the line.
      this.#addNotice(line, false);
    }
  }

  /* ----------------------------------------------------------------------- */
  /* Files of a project session                                               */
  /* ----------------------------------------------------------------------- */

  async refreshFiles(): Promise<void> {
    if (!this.active || this.scope !== "project") return;
    try {
      this.#setFiles((await api.files()).files);
    } catch (error) {
      reportError("live.files", error);
    }
  }

  #setFiles(files: readonly LiveFileNode[]): void {
    this.files = files;
    this.#inSession.clear();
    for (const f of files) this.#inSession.add(f.path);
  }

  /** The plan was applied (or a file arrived): list the files, bind the open ones. */
  async #afterFiles(): Promise<void> {
    await this.refreshFiles();
    await this.#attachOpen();
  }

  async #attachOpen(): Promise<void> {
    for (const doc of documents.all()) {
      if (this.isLive(doc.path)) await this.#attach(doc.path);
    }
  }

  async #attach(path: string): Promise<void> {
    const doc = documents.get(path);
    if (doc === undefined || this.#session === null) return;
    await this.#bindings.attach(doc);
  }

  #tree(ops: readonly LiveTreeOp[]): void {
    let files = [...this.files];
    for (const op of ops) {
      const by = this.#nameOf(op.by);
      if (op.op === "create") {
        if (!files.some((f) => f.path === op.path)) {
          files.push({ path: op.path, kind: "text", peers: [] });
        }
        this.deleted.delete(op.path);
        if (documents.get(op.path) !== undefined) void this.#attach(op.path);
      } else if (op.op === "delete") {
        files = files.filter((f) => f.path !== op.path);
        if (documents.get(op.path) !== undefined) this.deleted.set(op.path, by);
        void this.#bindings.detach(op.path, { quiet: true });
      } else {
        const from = op.from ?? "";
        files = files.map((f) => (f.path === from ? { ...f, path: op.path } : f));
        void this.#bindings.detach(from, { quiet: true });
        if (documents.get(op.path) !== undefined) void this.#attach(op.path);
      }
      this.#treeBatch.push(op);
    }
    this.#setFiles(files);
  }

  /** Decide the files of a joined project (phase 2 of the join). */
  async resolvePlan(choices: readonly LiveChoice[]): Promise<boolean> {
    if (this.plan === null) return false;
    try {
      const out = await api.resolve(choices);
      this.plan = null;
      if (out.trashed.length > 0) {
        notice.show(
          `${out.trashed.length} of your files were replaced by the host's. Your copies are in live-trash.`,
        );
      }
      await this.#afterFiles();
      return true;
    } catch (error) {
      notice.error(liveError(error).message);
      return false;
    }
  }

  /** Keep a file out of the session (it stays on this disk, unshared). */
  async exclude(path: string): Promise<void> {
    try {
      await this.#bindings.detach(path);
      await api.exclude(path);
      await this.refreshFiles();
    } catch (error) {
      notice.error(liveError(error).message);
    }
  }

  async include(path: string): Promise<void> {
    try {
      await api.include(path);
      await this.#afterFiles();
    } catch (error) {
      notice.error(liveError(error).message);
    }
  }

  /** Bring back a file someone else deleted. */
  async restore(path: string): Promise<void> {
    try {
      await api.restore(path);
      this.deleted.delete(path);
      await this.#afterFiles();
    } catch (error) {
      notice.error(liveError(error).message);
    }
  }

  /* ----------------------------------------------------------------------- */
  /* Follow                                                                   */
  /* ----------------------------------------------------------------------- */

  /** Follow a person (`null` stops): their file opens and scrolls to their caret. */
  follow(peer: string | null): void {
    if (peer === null) {
      this.#followEvent({ kind: "unfollow" });
      return;
    }
    if (!this.active || !this.peers.some((p) => p.peer === peer)) return;
    const at = this.peers.find((p) => p.peer === peer)?.path ?? null;
    const head = this.#heads.get(peer);
    this.#followEvent({
      kind: "follow",
      peer,
      viewing: tabs.active?.path ?? null,
      at:
        at === null
          ? null
          : {
              path: at,
              head: head !== undefined && head.path === at ? head.head : null,
            },
    });
  }

  #followEvent(event: FollowEvent): void {
    if (this.#follow.following === null && event.kind !== "follow") {
      if (event.kind === "navigated")
        this.#follow = followStep(this.#follow, event).state;
      return;
    }
    const step = followStep(this.#follow, event);
    this.#follow = step.state;
    if (step.state.following !== this.following) {
      this.following = step.state.following;
      void api.follow(this.following).catch(() => undefined);
    }
    if (step.ended !== null && step.ended !== "stopped") {
      notice.show(
        step.ended === "left"
          ? "The person you were following left the session."
          : "Stopped following: you moved on your own.",
      );
    }
    for (const effect of step.effects) void this.#run(effect);
  }

  async #run(effect: FollowEffect): Promise<void> {
    if (effect.kind === "open") {
      const { openFile } = await import("./actions");
      await openFile(effect.path, { focus: false });
    } else {
      this.#scrollTo(effect.path, effect.head, 12);
    }
  }

  /** Scroll `head` into view once the file is in front (it may be opening). */
  #scrollTo(path: string, head: number, tries: number): void {
    const view = documents.view;
    if (documents.shown?.path === path && view !== undefined) {
      const at = Math.max(0, Math.min(head, view.state.doc.length));
      view.dispatch({
        effects: EditorView.scrollIntoView(at, { y: "center" }),
        annotations: [fromLive.of(true), fromKernel.of(true)],
      });
      return;
    }
    if (tries > 0 && this.following !== null) {
      setTimeout(() => this.#scrollTo(path, head, tries - 1), 50);
    }
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

  /** Rebuild this copy from the others in the session (the Re-sync button). */
  async resync(): Promise<void> {
    if (!this.active) return;
    try {
      await api.resync();
      notice.show(
        "Re-syncing with the session. Your file is kept until someone answers.",
      );
    } catch (error) {
      notice.error(liveError(error).message);
    }
  }

  /**
   * Trailers and a message line for a version saved during a session: the
   * people who edited since the last version. Nothing when there is no session.
   */
  async coauthors(committer: {
    name: string | null;
    email: string | null;
  }): Promise<{ trailers: readonly string[]; withoutEmail: readonly string[] }> {
    if (!this.active) return { trailers: [], withoutEmail: [] };
    try {
      const out = await api.contributors(committer);
      return { trailers: out.trailers, withoutEmail: out.without_email };
    } catch (error) {
      reportError("live.contributors", error);
      return { trailers: [], withoutEmail: [] };
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

  /**
   * Share: the open file (`file`, the default) or the whole project
   * (`project`). Starts a session and puts the invite on the clipboard.
   */
  async share(scope: LiveScope = "file"): Promise<void> {
    if (this.active || this.busy) return;
    const doc = scope === "file" ? this.#target() : null;
    if (scope === "file" && doc === null) {
      notice.error("Open a text file to share it live.");
      return;
    }
    if (scope === "project" && workspace.project === null) {
      notice.error("Open a project to share it live.");
      return;
    }
    this.listen();
    this.busy = true;
    this.needsRelay = false;
    try {
      await this.#syncBrowser();
      if (doc !== null) {
        await doc.sync.idle();
        const docId = doc.sync.docId;
        if (docId === null) {
          notice.error("The document is not ready yet. Try again in a moment.");
          return;
        }
        const started = await api.start({ doc: docId, file: doc.path });
        this.#begin(doc, started.session, started.version, "host");
        this.inviteText = started.invite ?? null;
      } else {
        // The plugin reads what is not open from disk: open files are settled first.
        for (const open of documents.all()) await open.sync.idle();
        const started = await api.start({ scope: "project" });
        this.#beginProject(started.session, "host");
        this.inviteText = started.invite ?? null;
        await this.#afterFiles();
      }
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

  /** Ask for an invite and join what it names (a file, or a whole project). */
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
      let inspected;
      try {
        inspected = await api.inspect(text);
      } catch (error) {
        notice.error(liveError(error).message);
        return;
      }
      if (inspected.scope === "project") {
        await this.#joinProject(text);
        return;
      }
      await this.#joinFile(text, inspected.file);
    } catch (error) {
      notice.error(liveError(error).message);
    } finally {
      this.busy = false;
    }
  }

  /** Phase 1 of a project join: connect; the plan arrives as an event. */
  async #joinProject(invite: string): Promise<void> {
    const result = await api.join({ invite });
    this.#beginProject(result.session, "guest");
    notice.show("Joined the project. Choose what to do with each file.");
  }

  async #joinFile(text: string, file: string | null): Promise<void> {
    const doc = await this.#joinTarget(file);
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
    if (result.waiting === true) {
      this.connection = "waiting";
      this.#bindings.setReadOnly(true);
    }
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
    notice.show(
      result.waiting === true
        ? `Joined ${inviteFileLabel(result.file)}. Waiting for someone in the session to answer; your file is not changed yet.`
        : `Joined ${inviteFileLabel(result.file)}.`,
    );
  }

  /** The document an invite is for: the file it names if the project has it, else the open one. */
  async #joinTarget(file: string | null): Promise<DocSession | null> {
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

  #reset(session: string, role: "host" | "guest", scope: LiveScope): void {
    this.#session = session;
    this.role = role;
    this.scope = scope;
    this.connection = "connecting";
    this.attempt = 0;
    this.error = null;
    this.peers = [];
    this.files = [];
    this.#inSession.clear();
    this.plan = null;
    this.progress = null;
    this.deleted.clear();
    this.treeOps = [];
    this.notices = [];
    this.#heads.clear();
    this.#follow = NOT_FOLLOWING;
    this.following = null;
    this.#bindings.begin(session);
  }

  /** Replay what arrived before the session was known. */
  #replay(session: string): void {
    for (const event of this.#early.splice(0)) {
      if (event.payload.session === session) this.#deliver(event);
    }
  }

  #begin(
    doc: DocSession,
    session: string,
    version: number,
    role: "host" | "guest",
  ): void {
    this.#reset(session, role, "file");
    this.path = doc.path;
    this.#bindings.adopt(doc, session, version);
    this.#replay(session);
  }

  #beginProject(session: string, role: "host" | "guest"): void {
    this.#reset(session, role, "project");
    this.path = null;
    this.#replay(session);
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
      notice.show(
        this.scope === "project"
          ? "Invite copied. Anyone who has it can edit this project with you."
          : "Invite copied. Anyone who has it can edit this file with you.",
      );
    } catch (error) {
      if (this.inviteText === null) reportError("live.copyInvite", error);
      // The bar shows the invite in a field that is selected for copying.
      notice.show("Select the invite in the bar and copy it.");
    }
  }

  /** Leave: edits on their way get a moment to arrive, then the session ends. */
  async leave(): Promise<void> {
    if (this.#session === null) {
      this.#clear();
      return;
    }
    try {
      await this.#bindings.settle();
      await api.leave();
    } catch (error) {
      reportError("live.leave", error);
      notice.error(errorMessage(error));
    }
    await this.#bindings.closeAll();
    this.#clear();
    notice.show(
      this.scope === "project"
        ? "You left the live session. Your files stay as they are."
        : "You left the live session. The file is still yours to save.",
    );
  }

  /** The document went away (its tab closed): end the session without touching it. */
  async #abandon(): Promise<void> {
    try {
      await api.leave();
    } catch (error) {
      reportError("live.leave", error);
    }
    this.#bindings.abandon();
    this.#clear();
  }

  #clear(): void {
    this.#session = null;
    this.#early = [];
    this.#treeBatch.cancel();
    this.#peopleBatch.cancel();
    this.#heads.clear();
    this.#follow = NOT_FOLLOWING;
    this.connection = "off";
    this.role = null;
    this.scope = null;
    this.path = null;
    this.peers = [];
    this.files = [];
    this.#inSession.clear();
    this.plan = null;
    this.progress = null;
    this.following = null;
    this.deleted.clear();
    this.treeOps = [];
    this.notices = [];
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
