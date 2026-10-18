/**
 * Wording for the built-in LaTeX engine: sizes, the progress line, and plain
 * explanations of why a download failed. Pure, so it is unit-tested.
 */

import type { EngineProgress } from "$lib/kernel/api";

const MB = 1_000_000;

/** `14.4 MB`; one decimal under 100 MB, none above. */
export function formatSize(bytes: number): string {
  const mb = bytes / MB;
  return `${mb < 100 ? mb.toFixed(1) : Math.round(mb).toString()} MB`;
}

/** `about 14 MB`, for button labels. */
export function aboutSize(bytes: number): string {
  return `about ${Math.max(1, Math.round(bytes / MB))} MB`;
}

/** The label of every "get the engine" button. */
export function downloadLabel(bytes: number | undefined): string {
  return bytes === undefined
    ? "Download the built-in engine"
    : `Download the built-in engine (${aboutSize(bytes)})`;
}

/** What the progress area says. */
export function progressLabel(progress: EngineProgress | null): string {
  if (progress === null) return "Starting the download…";
  switch (progress.phase) {
    case "downloading":
      return `Downloading ${formatSize(progress.received)} of ${formatSize(progress.total)}`;
    case "verifying":
      return "Checking the download…";
    case "installing":
      return "Installing…";
    case "done":
      return "Done";
  }
}

/** 0 to 1, for the bar; the checking and installing steps show it full. */
export function progressFraction(progress: EngineProgress | null): number {
  if (progress === null || progress.total <= 0) return 0;
  if (progress.phase !== "downloading") return 1;
  return Math.min(1, Math.max(0, progress.received / progress.total));
}

export interface EngineFailure {
  readonly code: string | null;
  /** True when the user pressed Cancel: not an error to report. */
  readonly cancelled: boolean;
  /** Plain words for the dialog. */
  readonly message: string;
}

/** Turn a `[code] message` failure of `latex.engine-install` into plain words. */
export function explainEngineFailure(raw: string): EngineFailure {
  const match = /^\[([a-z_]+)\]\s*(.*)$/s.exec(raw);
  const code = match?.[1] ?? null;
  const detail = match?.[2] ?? raw;
  switch (code) {
    case "engine_cancelled":
      return { code, cancelled: true, message: "The download was cancelled." };
    case "engine_network":
      return {
        code,
        cancelled: false,
        message: `Manang could not reach the download server. Check your internet connection and try again. (${detail})`,
      };
    case "engine_truncated":
      return {
        code,
        cancelled: false,
        message:
          "The download stopped before it finished. Try again: it continues where it left off.",
      };
    case "engine_hash_mismatch":
      return {
        code,
        cancelled: false,
        message:
          "The downloaded file did not match its checksum, so Manang did not install it. Try again later.",
      };
    case "engine_too_large":
      return {
        code,
        cancelled: false,
        message:
          "The server sent more data than expected, so Manang stopped and installed nothing.",
      };
    case "engine_wont_run":
      return {
        code,
        cancelled: false,
        message: `The downloaded engine cannot start on this computer, so it was not installed. You can install TeX Live or MiKTeX instead. (${detail})`,
      };
    case "engine_not_published":
    case "engine_platform_unsupported":
      return {
        code,
        cancelled: false,
        message:
          "The built-in engine is not available for this version of Manang on this computer yet. Install TeX Live or MiKTeX instead.",
      };
    case "engine_busy":
      return {
        code,
        cancelled: false,
        message: "A download is already running.",
      };
    case "consent_required":
      return {
        code,
        cancelled: false,
        message: "Manang needs your permission to download the engine.",
      };
    default:
      return { code, cancelled: false, message: detail };
  }
}

/** Why the offer is not available, in a sentence, for Settings. */
export function unavailableReason(reason: string | null | undefined): string {
  switch (reason) {
    case "engine_platform_unsupported":
      return "There is no built-in engine for this computer yet. Install TeX Live or MiKTeX to compile LaTeX.";
    case "engine_not_published":
      return "The built-in engine is not available in this version of Manang yet. Install TeX Live or MiKTeX to compile LaTeX.";
    default:
      return "The built-in engine cannot be downloaded right now. Install TeX Live or MiKTeX to compile LaTeX.";
  }
}
