/**
 * Self-healing (`heal.*`): deterministic one-click fixes for small compile
 * errors. Typed against `crates/manang-heal` (`plugin.rs`). Edits are UTF-16
 * offsets of the text that was sent, the unit CodeMirror uses.
 */

import type { Kernel } from "./client";

export type HealEngine = "typst" | "latex" | "markdown";

/** `certain` fixes have one reading and may be applied in bulk; `likely` are the user's call. */
export type HealConfidence = "certain" | "likely";

export interface HealEdit {
  readonly from: number;
  readonly to: number;
  readonly insert: string;
}

export interface HealFix {
  /** Stable for the same fix, e.g. `latex.package:graphicx`. */
  readonly id: string;
  /** What the button says, e.g. `Add \usepackage{graphicx}`. */
  readonly label: string;
  /** One sentence on why. */
  readonly why: string;
  readonly edits: readonly HealEdit[];
  readonly confidence: HealConfidence;
  readonly rule: string;
  /** Indexes into the request's `diagnostics` that this fix clears. */
  readonly diagnostics: readonly number[];
  /** Index into `findings` for a problem no compiler reports (Markdown). */
  readonly finding?: number;
}

/** A problem found by scanning the text rather than reported by a compiler. */
export interface HealFinding {
  readonly message: string;
  readonly severity: "error" | "warning";
  readonly from: number;
  readonly to: number;
  /** 1-based line of `from`. */
  readonly line: number;
  readonly rule: string;
}

/** The certain fixes that do not overlap, as one set of edits. */
export interface HealFixAll {
  readonly label: string;
  readonly fixes: readonly string[];
  readonly edits: readonly HealEdit[];
}

export interface HealSuggestion {
  readonly fixes: readonly HealFix[];
  readonly fix_all?: HealFixAll;
  readonly findings: readonly HealFinding[];
  /** False when `heal.enabled` is off (then there are no fixes). */
  readonly enabled: boolean;
}

/** A compile diagnostic in the shape the compile commands return. */
export interface HealDiagnosticIn {
  readonly severity: "error" | "warning" | "info";
  readonly message: string;
  /** 1-based. */
  readonly line?: number | null;
  /** 1-based, in Unicode scalar values. */
  readonly column?: number | null;
  /** Scalar offsets, Typst only. */
  readonly range?: { readonly start: number; readonly end: number } | null;
  readonly hints?: readonly string[];
}

export interface HealRequest {
  readonly engine: HealEngine;
  readonly file: string;
  readonly text: string;
  readonly diagnostics: readonly HealDiagnosticIn[];
}

export function healApi(kernel: Kernel) {
  return {
    suggest: (request: HealRequest) =>
      kernel.run<HealSuggestion>("heal.suggest", { ...request }),
    configure: (values: Record<string, unknown>) =>
      kernel.run<Record<string, unknown>>("heal.configure", { values }),
  };
}

export type HealApi = ReturnType<typeof healApi>;
