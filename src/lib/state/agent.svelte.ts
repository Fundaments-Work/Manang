/**
 * The AI agent as the shell sees it: the Agent view's transcript, the composer, the status of the
 * provider, proposals and their inline diffs.
 *
 * Nothing here sends anything by itself. `send` runs only when the person presses Send (or picks
 * an AI action, or "Fix with AI"): there are no ambient or background calls. All agent logic
 * lives in Rust (`crates/manang-agent`); this class dispatches commands and folds the `agent.*`
 * events into view state (`$lib/agent/reducer`).
 *
 * One composer, one code path: a message is the author's text, an optional slash command
 * (`/rewrite`, `/fix`...), the selection and the active file, and whatever they @-mention.
 */

import { contextLine, parseSlash, slashMatches, wordCount } from "$lib/agent/context";
import { describeEdit, locateEdit } from "$lib/agent/proposal";
import {
  adopt,
  answerApproval,
  answerAsk,
  begin,
  decideProposal,
  failStart,
  initialState,
  pendingProposal,
  reduce,
  type AgentViewState,
  type TranscriptItem,
} from "$lib/agent/reducer";
import type {
  AgentStatus,
  ConnectionResult,
  Mention,
  RequestPreview,
  SessionMeta,
  SlashCommand,
  StartArgs,
  TextEdit,
  TurnContext,
  WireDiagnostic,
} from "$lib/agent/types";
import { undo } from "@codemirror/commands";

import {
  applyInline,
  clearInline,
  proposeInline,
  setInlineHandlers,
} from "$lib/editor/aiDiff";
import { errorMessage, inTauri, kernel, reportError } from "$lib/kernel";
import { agentApi } from "$lib/kernel/agentApi";
import type { Problem } from "$lib/workbench/problems";

import * as actions from "./actions";
import { build } from "./build.svelte";
import { dialog } from "./dialog.svelte";
import { documents } from "./documents.svelte";
import { layout } from "./layout.svelte";
import { notice } from "./notice.svelte";
import { settings } from "./settings.svelte";
import { settingsPage } from "./settingsPage.svelte";
import { tabs } from "./tabs.svelte";
import { workspace } from "./workspace.svelte";

const api = agentApi(kernel);

/** Setting ids the browser mock needs pushed (the desktop shell's settings sync does it). */
const SYNCED = [
  "agent.enabled",
  "agent.provider",
  "agent.model",
  "agent.base-url",
  "agent.offline",
  "agent.include-thinking",
  "agent.context-window",
] as const;

/** Lines shown around a problem, and most problems sent. */
const CONTEXT_LINES = 3;
const MAX_PROBLEMS = 40;

export type SetupReason =
  "disabled" | "key_missing" | "model_missing" | "offline_blocked" | "policy";

/** Commands that work on the selection (the menu items need one). */
const NEEDS_SELECTION: ReadonlySet<string> = new Set([
  "rewrite",
  "shorten",
  "translate",
  "explain",
  "cite",
]);

export class AgentState {
  view = $state.raw<AgentViewState>(initialState());
  status = $state.raw<AgentStatus | null>(null);
  input = $state("");
  /** What the author added with `@` for the next message. */
  mentions = $state.raw<readonly Mention[]>([]);
  /** The slash commands (built-ins, user files, project files). */
  commands = $state.raw<readonly SlashCommand[]>([]);
  /** The exact request, while "Show what will be sent" is open. */
  preview = $state.raw<RequestPreview | null>(null);
  previewError = $state("");
  previewOpen = $state(false);
  /** Why the view asks for set-up instead of sending, or null. */
  setup = $state<SetupReason | null>(null);

  /** Earlier chats of this project, newest first (the History menu). */
  history = $state.raw<readonly SessionMeta[]>([]);
  #listening = false;

  readonly pending = $derived(pendingProposal(this.view));
  /** The Agent view is ready when nothing blocks sending. */
  readonly ready: boolean = $derived(this.status?.ready === true);
  /** The quiet line under the composer: what travels with the next message. */
  readonly contextLine: string = $derived(
    contextLine({
      path: tabs.active?.path,
      selectionWords: documents.selectionWords,
      mentions: this.mentions,
    }),
  );
  /** The slash commands that fit what is typed so far (the menu under the composer). */
  readonly slashMenu: readonly SlashCommand[] = $derived(
    slashMatches(this.input, this.commands),
  );

  /* ----------------------------------------------------------------------- */
  /* Start-up                                                                 */
  /* ----------------------------------------------------------------------- */

  /** Subscribe to `agent.*` events once. Safe to call again. */
  listen(): void {
    if (this.#listening) return;
    this.#listening = true;
    setInlineHandlers({
      accept: (id) => void this.accept(id),
      reject: (id) => void this.reject(id),
    });
    void kernel.onEvent((event) => {
      if (!event.topic.startsWith("agent.")) return;
      this.#onEvent(event.topic, event.payload);
    });
  }

  #onEvent(topic: string, payload: unknown): void {
    this.view = reduce(this.view, topic, payload);
    if (topic === "agent.proposal") {
      const item = this.pending;
      if (item?.edit != null) void this.#showInline(item.proposalId, item.edit);
    }
    if (topic === "agent.done" || topic === "agent.error") void this.refreshStatus();
  }

  /** Pushes the agent settings to the plugin in the browser, where no shell does. */
  async #syncBrowser(): Promise<void> {
    if (inTauri()) return;
    await settings.load(workspace.project?.root ?? null);
    const values: Record<string, unknown> = {};
    for (const id of SYNCED) values[id] = settings.value(id);
    try {
      await api.configure(values);
    } catch (error) {
      reportError("The agent settings were not accepted", error);
    }
  }

  async refreshStatus(): Promise<void> {
    try {
      await this.#syncBrowser();
      this.status = await api.status(workspace.project?.root);
    } catch (error) {
      this.status = null;
      reportError("The agent status could not be read", error);
    }
  }

  async loadCommands(): Promise<void> {
    try {
      this.commands = (await api.commands(workspace.project?.root)).commands;
    } catch {
      this.commands = [];
    }
  }

  /** Why the agent cannot send right now, in the order the person fixes them. */
  #blocker(): SetupReason | null {
    const problems = this.status?.problems ?? [];
    for (const reason of [
      "disabled",
      "policy",
      "key_missing",
      "model_missing",
      "offline_blocked",
    ] as const) {
      if (problems.includes(reason)) return reason;
    }
    return null;
  }

  /** Opens the Agent column and focuses its composer (Edit > AI, shortcut). */
  reveal(): void {
    this.listen();
    layout.showAgent();
    void this.refreshStatus();
    void this.loadCommands();
  }

  /** Opens the Agent page of Settings (the setup card and the chip link here). */
  openSetup(): void {
    settingsPage.show("agent");
  }

  /** Chooses a provider (and resets the model to its default). */
  async chooseProvider(id: string): Promise<void> {
    await settings.set("agent.provider", id, "user");
    await settings.set("agent.model", "", "user");
    await this.refreshStatus();
  }

  async chooseModel(model: string): Promise<void> {
    await settings.set("agent.model", model, "user");
    await this.refreshStatus();
  }

  /** Turns the agent on again (the "off" state of the view). */
  async turnOn(): Promise<void> {
    await settings.set("agent.enabled", true, "user");
    await this.refreshStatus();
  }

  /* ----------------------------------------------------------------------- */
  /* History                                                                  */
  /* ----------------------------------------------------------------------- */

  async loadHistory(): Promise<void> {
    try {
      const out = await api.sessions(workspace.project?.root);
      this.history = [...out.sessions].sort((a, b) => b.updated_ms - a.updated_ms);
    } catch {
      this.history = [];
    }
  }

  /**
   * Shows an earlier chat read-only: the first request as its title and the assistant's text.
   * Sending starts a new chat.
   */
  async openSession(id: string): Promise<void> {
    const meta = this.history.find((h) => h.id === id);
    try {
      const out = await api.transcript(id, workspace.project?.root);
      const items: TranscriptItem[] = [
        { kind: "user", text: meta?.title ?? "Earlier chat", context: "" },
      ];
      for (const raw of out.messages) {
        const m = raw as {
          role?: string;
          content?: { type?: string; text?: string }[];
        };
        if (m.role !== "assistant") continue;
        const text = (m.content ?? [])
          .filter((b) => b.type === "text" && typeof b.text === "string")
          .map((b) => b.text)
          .join("\n\n");
        if (text !== "") items.push({ kind: "assistant", text, open: false });
      }
      items.push({
        kind: "notice",
        text: "An earlier chat. Send a message to start a new one.",
      });
      this.view = { ...initialState(), items };
      this.mentions = [];
      this.setup = null;
    } catch (error) {
      reportError("That chat could not be opened", error);
    }
  }

  /* ----------------------------------------------------------------------- */
  /* Context: what travels with a message                                     */
  /* ----------------------------------------------------------------------- */

  /** The editor selection, or null when nothing is selected. */
  #selection(): NonNullable<TurnContext["selection"]> | null {
    const view = documents.view;
    const shown = documents.shown;
    if (view === undefined || shown === undefined) return null;
    const range = view.state.selection.main;
    if (range.empty) return null;
    const doc = view.state.doc;
    return {
      path: shown.path,
      text: view.state.sliceDoc(range.from, range.to),
      line_from: doc.lineAt(range.from).number,
      line_to: doc.lineAt(range.to).number,
    };
  }

  #wireDiagnostic(problem: Problem): WireDiagnostic {
    const text = documents.get(problem.file)?.text() ?? "";
    const lines = text.split("\n");
    const line = problem.line ?? 1;
    const from = Math.max(0, line - 1 - CONTEXT_LINES);
    const context = lines.slice(from, line + CONTEXT_LINES).join("\n");
    return {
      path: problem.file,
      line,
      ...(problem.column === null ? {} : { column: problem.column }),
      severity: problem.severity,
      message: problem.message,
      ...(context === "" ? {} : { context }),
    };
  }

  /** The editor's current problems, as the `diagnostics` tool reports them. */
  #problems(): WireDiagnostic[] {
    return build.problems
      .filter((p) => p.file !== "")
      .slice(0, MAX_PROBLEMS)
      .map((p) => this.#wireDiagnostic(p));
  }

  /** The turn context for the next message. */
  #context(extra: TurnContext = {}): TurnContext {
    const active = tabs.active;
    const path = active?.path;
    const selection = this.#selection();
    const buffers = documents.dirtySources();
    const diagnostics = this.#problems();
    return {
      view: active?.kind === "figure" ? "figure" : "document",
      ...(path === undefined ? {} : { active: { path, line: documents.cursor.line } }),
      ...(selection === null ? {} : { selection }),
      ...(this.mentions.length === 0 ? {} : { mentions: this.mentions }),
      ...(Object.keys(buffers).length === 0 ? {} : { buffers }),
      ...(diagnostics.length === 0 ? {} : { diagnostics }),
      ...extra,
    };
  }

  /** Mentions a file: its open text travels with the message, else the host reads it. */
  mentionFile(path: string): void {
    if (this.mentions.some((m) => m.kind === "file" && m.path === path)) return;
    const doc = documents.get(path);
    this.mentions = [
      ...this.mentions,
      doc === undefined
        ? { kind: "file", path }
        : { kind: "file", path, text: doc.text() },
    ];
  }

  /** Mentions the current problems. */
  mentionProblems(): void {
    const list = this.#problems();
    if (list.length === 0) {
      notice.show("There are no problems to mention.");
      return;
    }
    this.mentions = [
      ...this.mentions.filter((m) => m.kind !== "problems"),
      { kind: "problems", diagnostics: list },
    ];
  }

  /** Mentions the open figure. */
  mentionFigure(): void {
    const active = tabs.active;
    if (active?.kind !== "figure") {
      notice.show("Open a figure first.");
      return;
    }
    this.mentions = [
      ...this.mentions.filter((m) => m.kind !== "figure"),
      { kind: "figure", path: active.path },
    ];
  }

  removeMention(index: number): void {
    this.mentions = this.mentions.filter((_, i) => i !== index);
  }

  /* ----------------------------------------------------------------------- */
  /* Sending                                                                  */
  /* ----------------------------------------------------------------------- */

  #args(
    prompt: string,
    command: string | null,
    context: TurnContext,
    extra: Partial<StartArgs> = {},
  ): StartArgs {
    const root = workspace.project?.root;
    return {
      ...(prompt.trim() === "" ? {} : { prompt: prompt.trim() }),
      ...(command === null ? {} : { command }),
      context,
      ...(root === undefined ? {} : { project_root: root }),
      ...extra,
    };
  }

  /** The first-use consent dialog. Resolves to whether the person agreed. */
  async #consent(): Promise<boolean> {
    const status = this.status;
    if (status === null) return false;
    const answer = await dialog.ask({
      title: `Send data to ${status.provider_name}?`,
      message: status.consent_text,
      actions: [{ id: "agree", label: "Agree", primary: true }],
    });
    if (answer !== "agree") return false;
    try {
      await api.consent(status.provider, true);
      await this.refreshStatus();
      return true;
    } catch (error) {
      reportError("Consent could not be recorded", error);
      return false;
    }
  }

  /** Sends the message in the box. A `/command` at the start runs that command. */
  async send(): Promise<void> {
    if (this.view.running) return;
    if (this.commands.length === 0) await this.loadCommands();
    const { command, prompt } = parseSlash(this.input, this.commands);
    if (command === null && prompt === "") return;
    await this.#run(prompt, command, this.view.sessionId);
    this.input = "";
  }

  /** Puts `/name ` in the box (the empty-state rows) and focuses it. */
  fillCommand(name: string): void {
    this.input = `/${name} `;
    document.querySelector<HTMLElement>("[data-agent-input]")?.focus();
  }

  /** Sends `text` as a plain message in the open chat (an answer to a question, "continue"). */
  async sendText(text: string): Promise<void> {
    if (this.view.running) return;
    await this.#run(text, null, this.view.sessionId);
  }

  /** The author's answer to the model's question: a choice or typed text. */
  async answer(index: number, text: string): Promise<void> {
    this.view = answerAsk(this.view, index, text);
    await this.sendText(text);
  }

  /** What the transcript shows for the author's message. */
  #shown(prompt: string, command: string | null): string {
    if (command === null) return prompt;
    return prompt === "" ? `/${command}` : `/${command} ${prompt}`;
  }

  /**
   * Starts a message. `sessionId` continues a conversation, null starts a fresh one. Checks the
   * set-up first: a missing key opens the set-up state, a missing consent asks for it.
   */
  async #run(
    prompt: string,
    command: string | null,
    sessionId: string | null,
    context: TurnContext = this.#context(),
  ): Promise<void> {
    this.listen();
    await this.refreshStatus();
    const blocker = this.#blocker();
    if (blocker !== null) {
      this.setup = blocker;
      return;
    }
    this.setup = null;
    if (this.status?.consent !== true && !(await this.#consent())) return;
    const base = sessionId === null ? initialState() : this.view;
    this.view = begin(
      base,
      this.#shown(prompt, command),
      contextLine({
        path: context.active?.path,
        selectionWords:
          context.selection === undefined ? 0 : wordCount(context.selection.text),
        mentions: context.mentions ?? [],
      }),
    );
    this.mentions = [];
    try {
      const out = await api.start(
        this.#args(prompt, command, context, {
          ...(sessionId === null ? {} : { session_id: sessionId }),
        }),
      );
      this.view = adopt(this.view, out.session_id);
    } catch (error) {
      this.view = failStart(this.view, errorMessage(error));
      void this.refreshStatus();
    }
  }

  /** Runs the conversation again as it stands after a connection error (nothing is repeated). */
  async retry(): Promise<void> {
    const id = this.view.sessionId;
    if (id === null || this.view.running) return;
    const items = [...this.view.items];
    while (items.length > 0 && items[items.length - 1]?.kind === "error") items.pop();
    this.view = { ...this.view, items, running: true, stopReason: null };
    try {
      await api.start(this.#args("", null, {}, { session_id: id, retry: true }));
    } catch (error) {
      this.view = failStart(this.view, errorMessage(error));
    }
  }

  async stop(): Promise<void> {
    const id = this.view.sessionId;
    if (id === null || !this.view.running) return;
    try {
      await api.cancel(id);
    } catch (error) {
      reportError("The request could not be stopped", error);
    }
  }

  /** A fresh conversation. */
  newConversation(): void {
    this.view = initialState();
    this.mentions = [];
    this.setup = null;
    clearInlineEverywhere();
  }

  /* ----------------------------------------------------------------------- */
  /* Menu actions and "Fix with AI": the same slash commands                    */
  /* ----------------------------------------------------------------------- */

  /** Runs a slash command on the editor selection (Edit > AI, the empty-state rows). */
  async runCommand(name: string, args = ""): Promise<void> {
    if (NEEDS_SELECTION.has(name) && this.#selection() === null) {
      layout.showAgent();
      notice.show("Select some text in the editor first.");
      return;
    }
    let prompt = args;
    if (name === "translate" && prompt.trim() === "") {
      const answer = await dialog.prompt({
        title: "Translate the selection",
        message: "Which language should the selection be translated into?",
        input: { label: "Target language", value: "English" },
        actions: [{ id: "go", label: "Translate", primary: true }],
      });
      if (answer.id !== "go" || answer.value.trim() === "") return;
      prompt = answer.value.trim();
    }
    layout.showAgent();
    void this.loadCommands();
    await this.#run(prompt, name, null);
  }

  /** "Fix with AI" on a Problems entry: the diagnostic and the lines around it, nothing else. */
  async fixProblem(problem: Problem): Promise<void> {
    if (problem.file === "") return;
    if (documents.get(problem.file) === undefined) {
      if (!(await actions.openFile(problem.file, { focus: false }))) return;
    }
    layout.showAgent();
    void this.loadCommands();
    const diagnostic = this.#wireDiagnostic(problem);
    await this.#run("", "fix", null, {
      view: "document",
      active: { path: problem.file, line: problem.line ?? 1 },
      diagnostics: [diagnostic],
      mentions: [{ kind: "problems", diagnostics: [diagnostic] }],
      ...(Object.keys(documents.dirtySources()).length === 0
        ? {}
        : { buffers: documents.dirtySources() }),
    });
  }

  /** Undoes an accepted proposal in the editor (the same as Ctrl+Z) and marks its row. */
  undoAccepted(id: string): void {
    const view = documents.view;
    if (view === undefined) {
      notice.show("Open the document, then press Ctrl+Z to undo.");
      return;
    }
    undo(view);
    this.view = decideProposal(this.view, id, "undone");
    view.focus();
  }

  /** Accepts every waiting proposal, one editor transaction each (so each undoes alone). */
  async acceptAll(): Promise<void> {
    const ids = this.view.items.flatMap((i) =>
      i.kind === "proposal" && i.state === "pending" ? [i.proposalId] : [],
    );
    for (const id of ids) await this.accept(id);
  }

  /* ----------------------------------------------------------------------- */
  /* Show what will be sent                                                    */
  /* ----------------------------------------------------------------------- */

  async previewRequest(): Promise<void> {
    this.listen();
    this.previewError = "";
    this.preview = null;
    try {
      await this.#syncBrowser();
      if (this.commands.length === 0) await this.loadCommands();
      const { command, prompt } = parseSlash(this.input, this.commands);
      const sessionId = this.view.sessionId;
      this.preview = await api.previewRequest(
        this.#args(
          prompt === "" && command === null ? "(your message)" : prompt,
          command,
          this.#context(),
          sessionId === null ? {} : { session_id: sessionId },
        ),
      );
    } catch (error) {
      this.previewError = errorMessage(error);
    }
    this.previewOpen = true;
  }

  closePreview(): void {
    this.previewOpen = false;
  }

  /* ----------------------------------------------------------------------- */
  /* Approvals and proposals                                                   */
  /* ----------------------------------------------------------------------- */

  async approve(
    callId: string,
    decision: "allow" | "allow_session" | "deny",
  ): Promise<void> {
    try {
      await api.approve(callId, decision);
      this.view = answerApproval(this.view, callId, decision !== "deny");
    } catch (error) {
      reportError("The answer was not delivered", error);
    }
  }

  /** The document of `edit`, shown in the editor; its text is what the diff is computed on. */
  async #documentFor(
    path: string,
  ): Promise<{ text: string; view: NonNullable<typeof documents.view> } | null> {
    if (documents.shown?.path !== path) {
      if (!(await actions.openFile(path, { focus: false }))) return null;
      // The editor pane swaps the state in on the next tick.
      await new Promise((resolve) => setTimeout(resolve, 0));
    }
    const view = documents.view;
    if (view === undefined || documents.shown?.path !== path) return null;
    return { text: view.state.doc.toString(), view };
  }

  async #showInline(id: string, edit: TextEdit): Promise<void> {
    const target = await this.#documentFor(edit.path);
    if (target === null) return;
    const changes = locateEdit(target.text, edit);
    if (changes === null) {
      await this.#stale(id);
      return;
    }
    proposeInline(target.view, { id, hunks: changes, explanation: edit.note });
  }

  async #stale(id: string): Promise<void> {
    this.view = decideProposal(this.view, id, "stale");
    notice.show("The text changed since the proposal was made, so it was dropped.");
    try {
      await api.rejectProposal(id);
    } catch (error) {
      reportError("The proposal could not be dropped", error);
    }
  }

  /** Accept: ONE editor transaction for all hunks (one Ctrl+Z), then the normal compile runs. */
  async accept(id: string): Promise<void> {
    const item = this.view.items.find(
      (i) => i.kind === "proposal" && i.proposalId === id,
    );
    if (item?.kind !== "proposal" || item.edit === null || item.state !== "pending")
      return;
    const edit = item.edit;
    const target = await this.#documentFor(edit.path);
    if (target === null) return;
    if (locateEdit(target.text, edit) === null) {
      clearInline(target.view, id);
      await this.#stale(id);
      return;
    }
    try {
      await api.applyProposal(id);
    } catch (error) {
      reportError("The proposal could not be accepted", error);
      return;
    }
    // The text may have moved while the command ran: locate again, then apply synchronously.
    const now = documents.view;
    if (now === undefined) return;
    const changes = locateEdit(now.state.doc.toString(), edit);
    if (changes === null) {
      clearInline(now, id);
      this.view = decideProposal(this.view, id, "stale");
      return;
    }
    applyInline(now, changes, id);
    this.view = decideProposal(this.view, id, "accepted");
    notice.show(`${describeEdit(edit)} applied. Ctrl+Z undoes it.`);
    now.focus();
  }

  async reject(id: string): Promise<void> {
    try {
      await api.rejectProposal(id);
    } catch (error) {
      reportError("The proposal could not be rejected", error);
      return;
    }
    if (documents.view !== undefined) clearInline(documents.view, id);
    this.view = decideProposal(this.view, id, "rejected");
    documents.focusEditor();
  }

  /** Brings the proposal's inline diff into view in the editor ("Review"). */
  async review(id: string): Promise<void> {
    const item = this.view.items.find(
      (i) => i.kind === "proposal" && i.proposalId === id,
    );
    if (item?.kind !== "proposal" || item.edit === null || item.state !== "pending")
      return;
    await this.#showInline(id, item.edit);
    documents.focusEditor();
  }

  /* ----------------------------------------------------------------------- */
  /* Provider and key (Settings)                                              */
  /* ----------------------------------------------------------------------- */

  /** Saves a key. It is never shown again, only whether one is stored. */
  async saveKey(provider: string, key: string): Promise<string> {
    try {
      await api.keySet(provider, key);
      await this.refreshStatus();
      return "";
    } catch (error) {
      return errorMessage(error);
    }
  }

  async clearKey(provider: string): Promise<void> {
    try {
      await api.keyClear(provider);
      await this.refreshStatus();
    } catch (error) {
      reportError("The key could not be removed", error);
    }
  }

  async testConnection(): Promise<ConnectionResult> {
    try {
      await this.#syncBrowser();
      return await api.testConnection(workspace.project?.root);
    } catch (error) {
      return { ok: false, message: errorMessage(error) };
    }
  }

  /** Records consent to send data to the provider (the last set-up step). */
  async grantConsent(provider: string): Promise<void> {
    try {
      await api.consent(provider, true);
      await this.refreshStatus();
    } catch (error) {
      reportError("Consent could not be recorded", error);
    }
  }

  async revokeConsent(provider: string): Promise<void> {
    try {
      await api.consent(provider, false);
      await this.refreshStatus();
    } catch (error) {
      reportError("Consent could not be withdrawn", error);
    }
  }
}

function clearInlineEverywhere(): void {
  if (documents.view !== undefined) clearInline(documents.view);
}

export const agent = new AgentState();
