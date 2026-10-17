/**
 * The shell's session with the kernel.
 *
 * Holds only what the shell must render: the sync status, the last notice and
 * the kernel's diagnostics. The document itself lives in Rust; `KernelSync`
 * keeps the editor's working copy aligned with it.
 *
 * Kernel-originated changes (another plugin writing the active buffer) are
 * detected by comparing revisions: `sync.check()` runs when the window regains
 * focus and after the shell runs a kernel command. When the Rust side gains a
 * push channel (a Tauri `Channel` or an `editor://changed` event) it should
 * simply call `session.refresh()`; nothing else changes.
 */

import type { EditorView } from "@codemirror/view";

import type { Diagnostic } from "$lib/editor/diagnostics";
import { applyKernelText } from "$lib/editor/kernelSync";
import { kernel, type KernelCommand } from "$lib/kernel";
import { LINT_COMMAND, fetchLint, toEditorDiagnostics } from "$lib/kernel/lint";
import { KernelSync, type SyncStatus } from "$lib/kernel/sync";

const LINT_DEBOUNCE_MS = 250;

export class Session {
  status = $state<SyncStatus>("idle");

  /** Last kernel failure or command outcome, shown quietly in the status bar. */
  notice = $state("");

  /** Kernel lint results, already placed in the current text. */
  lint = $state.raw<Diagnostic[]>([]);

  readonly sync: KernelSync;

  #view: EditorView | undefined;
  #lintTimer: ReturnType<typeof setTimeout> | undefined;
  #commands: Promise<KernelCommand[]> | undefined;

  constructor() {
    this.sync = new KernelSync(kernel, {
      applyText: (text) => {
        if (this.#view !== undefined) applyKernelText(this.#view, text);
      },
      onStatus: (status, detail) => {
        this.status = status;
        if (detail !== undefined) this.notice = detail;
      },
    });
  }

  /** Commands the kernel offers; fetched once and shared. */
  commands(): Promise<KernelCommand[]> {
    this.#commands ??= kernel.commands().catch(() => []);
    return this.#commands;
  }

  /**
   * Bind to the editor view and seed the kernel document with its text.
   * Returns a detach function for the component's cleanup.
   */
  attach(view: EditorView): () => void {
    this.#view = view;
    void this.sync.start(view.state.doc.toString()).then(() => this.scheduleLint());

    const onFocus = (): void => void this.refresh();
    window.addEventListener("focus", onFocus);

    return () => {
      window.removeEventListener("focus", onFocus);
      clearTimeout(this.#lintTimer);
      if (this.#view === view) this.#view = undefined;
    };
  }

  /** Pull in anything the kernel changed behind the editor's back. */
  async refresh(): Promise<void> {
    await this.sync.check();
    this.scheduleLint();
  }

  /** Run a kernel command picked from the palette. */
  async run(id: string): Promise<void> {
    try {
      await kernel.run(id);
      this.notice = "";
    } catch (error) {
      this.notice = `${id}: ${messageOf(error)}`;
    }
    await this.refresh();
  }

  /** Debounced: re-lint once typing settles and the kernel has the edits. */
  scheduleLint(): void {
    clearTimeout(this.#lintTimer);
    this.#lintTimer = setTimeout(() => void this.#lint(), LINT_DEBOUNCE_MS);
  }

  async #lint(): Promise<void> {
    const available = (await this.commands()).some((c) => c.id === LINT_COMMAND);
    if (!available || this.#view === undefined) return;
    await this.sync.idle();
    const items = await fetchLint(kernel);
    const view = this.#view;
    if (view === undefined) return;
    this.lint = toEditorDiagnostics(view.state.doc.toString(), items);
  }
}

function messageOf(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}

export const session = new Session();
