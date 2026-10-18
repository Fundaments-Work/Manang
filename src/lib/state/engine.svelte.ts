/**
 * The optional built-in LaTeX engine: its status, and the consent dialog that
 * downloads it. Nothing is downloaded until the person presses Download in the
 * dialog (`confirm`); every other entry point only opens the dialog.
 */

import { errorMessage, kernel, reportError } from "$lib/kernel";
import {
  latexApi,
  type BuiltinEngineStatus,
  type EngineProgress,
} from "$lib/kernel/api";
import { explainEngineFailure } from "$lib/workbench/engine";

import { build } from "./build.svelte";

const latex = latexApi(kernel);

export type EngineDialogStep = "closed" | "consent" | "running" | "done" | "failed";

export class EngineState {
  /** Null until first asked, or when the kernel has no LaTeX plugin. */
  status = $state.raw<BuiltinEngineStatus | null>(null);
  step = $state<EngineDialogStep>("closed");
  progress = $state.raw<EngineProgress | null>(null);
  /** Plain words for the failed step. */
  error = $state("");
  /** Set while `remove` runs. */
  removing = $state(false);

  /** Offer the download: the kernel can fetch it and it is not installed. */
  get offered(): boolean {
    const s = this.status;
    return s !== null && s.downloadable && !s.installed;
  }

  /** Read the status from the kernel. Never throws. */
  async refresh(): Promise<void> {
    try {
      this.status = await latex.engineStatus();
    } catch {
      // An older kernel without the command: there is no built-in engine.
      this.status = null;
    }
  }

  /** Show the consent dialog (after reading the current numbers). */
  async openOffer(): Promise<void> {
    await this.refresh();
    if (this.status === null || !this.status.downloadable || this.status.installed) {
      return;
    }
    this.error = "";
    this.progress = null;
    this.step = "consent";
  }

  /** The person pressed Download. The only place a download starts. */
  async confirm(): Promise<void> {
    if (this.step !== "consent" && this.step !== "failed") return;
    this.step = "running";
    this.progress = null;
    this.error = "";
    let unlisten: (() => void) | undefined;
    try {
      unlisten = await latex.onEngineProgress((p) => {
        if (this.step === "running") this.progress = p;
      });
      this.status = await latex.engineInstall(true);
      this.step = "done";
      // Compile again: the engine just became available.
      void build.compile(true);
    } catch (error) {
      const failure = explainEngineFailure(errorMessage(error));
      if (failure.cancelled) {
        this.step = "closed";
      } else {
        this.error = failure.message;
        this.step = "failed";
        reportError("engine.install", error);
      }
      await this.refresh();
    } finally {
      unlisten?.();
    }
  }

  /** Stop a running download. */
  async cancel(): Promise<void> {
    if (this.step !== "running") {
      this.close();
      return;
    }
    try {
      await latex.engineCancel();
    } catch (error) {
      reportError("engine.cancel", error);
    }
  }

  close(): void {
    if (this.step === "running") return;
    this.step = "closed";
  }

  /** Delete the engine; the next compile without TeX offers it again. */
  async remove(): Promise<void> {
    this.removing = true;
    try {
      this.status = await latex.engineRemove();
      void build.compile(true);
    } catch (error) {
      reportError("engine.remove", error);
      await this.refresh();
    } finally {
      this.removing = false;
    }
  }
}

export const engine = new EngineState();
