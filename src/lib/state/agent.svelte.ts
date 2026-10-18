/**
 * The AI agent as the shell sees it: the Agent view's transcript, the context tray, the status
 * of the provider, proposals and their inline diffs.
 *
 * Nothing here sends anything by itself. `send` runs only when the person presses Send (or
 * picks a selection action, or "Fix with AI"): there are no ambient or background calls. All
 * agent logic lives in Rust (`crates/manang-agent`); this class dispatches commands and folds
 * the `agent.*` events into view state (`$lib/agent/reducer`).
 */

import {
  clearUnpinned,
  diagnosticsChip,
  fileChip,
  mergeTray,
  removeChip,
  selectionChip,
  togglePin,
  toWire,
  trayChip,
  totalTokens,
} from "$lib/agent/chips";
import { describeEdit, locateEdit, minimalChange } from "$lib/agent/proposal";
import {
  adopt,
  answerApproval,
  begin,
  decideProposal,
  failStart,
  initialState,
  pendingProposal,
  reduce,
  type AgentViewState,
} from "$lib/agent/reducer";
import type {
  AgentFeature,
  AgentStatus,
  ChipKind,
  ConnectionResult,
  RequestPreview,
  StartArgs,
  TextEdit,
  TrayChip,
} from "$lib/agent/types";
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
  "agent.max-context-tokens",
] as const;

/** Lines shown around a problem. */
const CONTEXT_LINES = 3;

export type SetupReason =
  "disabled" | "key_missing" | "model_missing" | "offline_blocked" | "policy";

export class AgentState {
  view = $state.raw<AgentViewState>(initialState());
  status = $state.raw<AgentStatus | null>(null);
  tray = $state.raw<readonly TrayChip[]>([]);
  feature = $state<AgentFeature>("chat");
  input = $state("");
  /** The exact request, while "Preview what will be sent" is open. */
  preview = $state.raw<RequestPreview | null>(null);
  previewError = $state("");
  previewOpen = $state(false);
  /** Why the view asks for set-up instead of sending, or null. */
  setup = $state<SetupReason | null>(null);

  #listening = false;

  readonly trayTokens: number = $derived(totalTokens(this.tray));
  readonly pending = $derived(pendingProposal(this.view));
  /** The Agent view is ready when nothing blocks sending. */
  readonly ready: boolean = $derived(this.status?.ready === true);

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

  /** Opens the provider settings (the one-click "Set up"). */
  openSetup(): void {
    settingsPage.show();
  }

  /* ----------------------------------------------------------------------- */
  /* The context tray                                                         */
  /* ----------------------------------------------------------------------- */

  /** The editor selection as a chip, or null when nothing is selected. */
  selectionChip(): TrayChip | null {
    const view = documents.view;
    const shown = documents.shown;
    if (view === undefined || shown === undefined) return null;
    const range = view.state.selection.main;
    if (range.empty) return null;
    return selectionChip(
      shown.path,
      range.from,
      range.to,
      view.state.sliceDoc(range.from, range.to),
    );
  }

  /** Attaches the current selection if the tray has none (the default context). */
  attachSelection(): void {
    if (this.tray.some((c) => c.kind === "selection")) return;
    const chip = this.selectionChip();
    if (chip !== null) this.tray = mergeTray(this.tray, [chip]);
  }

  /** Adds one chip of `kind` for the active document. */
  async addChip(kind: ChipKind): Promise<void> {
    const active = tabs.active;
    const path = active?.path;
    let chip: TrayChip | null = null;
    switch (kind) {
      case "selection":
        chip = this.selectionChip();
        if (chip === null) notice.show("Select some text in the editor first.");
        break;
      case "file": {
        const doc = path === undefined ? undefined : documents.get(path);
        if (path === undefined || doc === undefined)
          notice.show("Open a document first.");
        else chip = fileChip(path, doc.text());
        break;
      }
      case "diagnostics":
        chip = this.#problemsChip(path);
        if (chip === null) notice.show("There are no problems to attach.");
        break;
      case "outline":
        if (path === undefined) notice.show("Open a document first.");
        else chip = trayChip({ kind: "outline", path });
        break;
      case "bibliography-keys":
        chip = trayChip({ kind: "bibliography-keys" });
        break;
      case "figure":
        if (active?.kind === "figure" && path !== undefined) {
          chip = trayChip({ kind: "figure", path });
        } else notice.show("Open a figure first.");
        break;
      case "pinned": {
        const answer = await dialog.prompt({
          title: "Pin a note",
          message: "The note is sent with every request until you remove it.",
          input: { label: "Note", value: "" },
          actions: [{ id: "pin", label: "Pin", primary: true }],
        });
        if (answer.id === "pin" && answer.value.trim() !== "") {
          chip = trayChip({ kind: "pinned", text: answer.value.trim(), pinned: true });
        }
        break;
      }
    }
    if (chip !== null) this.tray = mergeTray(this.tray, [chip]);
  }

  #problemsChip(path: string | undefined): TrayChip | null {
    const list = build.problems.filter(
      (p) => p.file !== "" && (path === undefined || p.file === path),
    );
    if (list.length === 0) return null;
    return diagnosticsChip(list.slice(0, 20).map((p) => this.#wireDiagnostic(p)));
  }

  #wireDiagnostic(problem: Problem) {
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

  removeChip(id: string): void {
    this.tray = removeChip(this.tray, id);
  }

  togglePin(id: string): void {
    this.tray = togglePin(this.tray, id);
  }

  /* ----------------------------------------------------------------------- */
  /* Sending                                                                  */
  /* ----------------------------------------------------------------------- */

  #args(
    feature: AgentFeature,
    prompt: string,
    extra: Partial<StartArgs> = {},
  ): StartArgs {
    const root = workspace.project?.root;
    return {
      feature,
      ...(prompt.trim() === "" ? {} : { prompt: prompt.trim() }),
      context: this.tray.map(toWire),
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

  /** Sends the message in the box. Continues the conversation when one is open. */
  async send(): Promise<void> {
    const text = this.input;
    if (this.view.running) return;
    const continuing = this.view.sessionId !== null && this.feature === "chat";
    await this.#run(this.feature, text, continuing ? this.view.sessionId : null);
    this.input = "";
  }

  /**
   * Starts a request. `sessionId` continues a conversation, null starts a fresh one. Checks the
   * set-up first: a missing key opens the set-up state, a missing consent asks for it.
   */
  async #run(
    feature: AgentFeature,
    prompt: string,
    sessionId: string | null,
    options?: StartArgs["options"],
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
    const shown = prompt.trim() !== "" ? prompt.trim() : defaultLine(feature);
    const base = sessionId === null ? initialState() : this.view;
    this.view = begin(
      base,
      shown,
      this.tray.map((c) => c.label),
    );
    try {
      const out = await api.start(
        this.#args(feature, prompt, {
          ...(sessionId === null ? {} : { session_id: sessionId }),
          ...(options === undefined ? {} : { options }),
        }),
      );
      this.view = adopt(this.view, out.session_id);
    } catch (error) {
      this.view = failStart(this.view, errorMessage(error));
      void this.refreshStatus();
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

  /** A fresh conversation; pinned chips stay. */
  newConversation(): void {
    this.view = initialState();
    this.tray = clearUnpinned(this.tray);
    this.setup = null;
    clearInlineEverywhere();
  }

  /* ----------------------------------------------------------------------- */
  /* Selection actions and "Fix with AI"                                       */
  /* ----------------------------------------------------------------------- */

  /** Rewrite, shorten, translate or explain the editor selection. */
  async runSelection(
    feature: "rewrite" | "shorten" | "translate" | "explain",
  ): Promise<void> {
    const chip = this.selectionChip();
    if (chip === null) {
      notice.show("Select some text in the editor first.");
      return;
    }
    let target: string | undefined;
    if (feature === "translate") {
      const answer = await dialog.prompt({
        title: "Translate the selection",
        message: "Which language should the selection be translated into?",
        input: { label: "Target language", value: "English" },
        actions: [{ id: "go", label: "Translate", primary: true }],
      });
      if (answer.id !== "go" || answer.value.trim() === "") return;
      target = answer.value.trim();
    }
    layout.showPanel("agent");
    this.feature = feature;
    this.tray = mergeTray(
      this.tray.filter((c) => c.pinned === true),
      [chip],
    );
    await this.#run(
      feature,
      "",
      null,
      target === undefined ? undefined : { target_language: target },
    );
  }

  /** "Fix with AI" on a Problems entry: the diagnostic and the lines around it, nothing else. */
  async fixProblem(problem: Problem): Promise<void> {
    if (problem.file === "") return;
    if (documents.get(problem.file) === undefined) {
      if (!(await actions.openFile(problem.file, { focus: false }))) return;
    }
    layout.showPanel("agent");
    this.feature = "fix-diagnostic";
    this.tray = mergeTray(
      this.tray.filter((c) => c.pinned === true),
      [diagnosticsChip([this.#wireDiagnostic(problem)])],
    );
    await this.#run("fix-diagnostic", "", null);
  }

  /* ----------------------------------------------------------------------- */
  /* Preview what will be sent                                                 */
  /* ----------------------------------------------------------------------- */

  async previewRequest(): Promise<void> {
    this.listen();
    this.previewError = "";
    this.preview = null;
    try {
      await this.#syncBrowser();
      this.preview = await api.previewRequest(this.#args(this.feature, this.input));
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
    const at = locateEdit(target.text, edit);
    if (at === null) {
      await this.#stale(id);
      return;
    }
    const change = minimalChange(edit, at);
    proposeInline(target.view, {
      id,
      from: change.from,
      to: change.to,
      insert: change.insert,
      explanation: edit.explanation,
    });
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

  /** Accept: ONE editor transaction (one Ctrl+Z), then the normal compile runs on its own. */
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
    const at = locateEdit(now.state.doc.toString(), edit);
    if (at === null) {
      clearInline(now, id);
      this.view = decideProposal(this.view, id, "stale");
      return;
    }
    applyInline(now, minimalChange(edit, at), id);
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

function defaultLine(feature: AgentFeature): string {
  switch (feature) {
    case "rewrite":
      return "Rewrite the selection";
    case "shorten":
      return "Shorten the selection";
    case "translate":
      return "Translate the selection";
    case "explain":
      return "Explain the selection";
    case "fix-diagnostic":
      return "Fix the problem";
    case "chat":
      return "Help me with this";
  }
}

export const agent = new AgentState();
