/**
 * Send feedback and Report a problem: a short dialog where the person says what
 * happened and reads the diagnostics report before copying it. Manang sends
 * nothing; the text goes wherever the person pastes it.
 */

import { diagnosticsReport } from "$lib/kernel/native";
import { reportError } from "$lib/kernel";
import { feedbackText, type FeedbackKind } from "$lib/workbench/feedback";

class FeedbackStore {
  open = $state(false);
  kind = $state<FeedbackKind>("feedback");
  message = $state("");
  includeReport = $state(true);
  report = $state("");
  loading = $state(false);
  failed = $state(false);
  copied = $state(false);

  async show(kind: FeedbackKind): Promise<void> {
    this.kind = kind;
    this.message = "";
    this.includeReport = true;
    this.copied = false;
    this.open = true;
    await this.#loadReport();
  }

  close(): void {
    this.open = false;
  }

  /** The text that would be copied right now. */
  get text(): string {
    return feedbackText(this.kind, this.message, this.includeReport ? this.report : "");
  }

  async copy(): Promise<boolean> {
    try {
      await navigator.clipboard.writeText(this.text);
      this.copied = true;
      return true;
    } catch (error) {
      reportError("feedback.copy", error);
      return false;
    }
  }

  async #loadReport(): Promise<void> {
    this.loading = true;
    this.failed = false;
    try {
      this.report = await diagnosticsReport();
    } catch (error) {
      reportError("feedback.report", error);
      this.report = "";
      this.failed = true;
    } finally {
      this.loading = false;
    }
  }
}

export const feedback = new FeedbackStore();
