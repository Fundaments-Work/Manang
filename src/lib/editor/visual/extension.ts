/**
 * The Visual editor as one CodeMirror extension (ADR 0015).
 *
 * Source stays the only truth: this decorates the real text and turns toolbar
 * actions into ordinary transactions. It is loaded on demand (a dynamic import
 * of this module) the first time a document is switched to Visual.
 */

import { isolateHistory } from "@codemirror/commands";
import {
  EditorSelection,
  Prec,
  type Extension,
  type TransactionSpec,
} from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";

import { planFits } from "./client";
import { visualField } from "./field";
import type { VisualHost } from "./host";
import type { EditPlan } from "./protocol";
import { createSync, visualHandle } from "./sync";
import { visualTheme } from "./theme";

export { onVisualActivity, visualHandle, type VisualHandle } from "./sync";
export type { VisualHost, VisualStatus } from "./host";

/** A plan as one transaction: one undo step, kept apart from typing. */
export function planToSpec(plan: EditPlan): TransactionSpec {
  return {
    changes: plan.changes.map((c) => ({ from: c.from, to: c.to, insert: c.insert })),
    ...(plan.selection === null
      ? {}
      : { selection: EditorSelection.single(plan.selection.from, plan.selection.to) }),
    annotations: [isolateHistory.of("full")],
    userEvent: "input.format",
    scrollIntoView: true,
  };
}

/**
 * Run a formatting command (`bold`, `heading` with `"2"`, ...) on the selection:
 * ask the session for a plan and dispatch it. False when nothing was applied.
 */
export async function runVisualCommand(
  view: EditorView,
  command: string,
  args?: string,
): Promise<boolean> {
  const handle = visualHandle(view);
  if (handle === null) return false;
  const plan = await handle.plan(command, args);
  if (plan === null || !planFits(plan, view.state.doc.length)) return false;
  if (plan.changes.length === 0 && plan.selection === null) return false;
  view.dispatch(planToSpec(plan));
  view.focus();
  return true;
}

/** Ctrl+B and Ctrl+I as in any word processor, in Visual mode only. */
const visualKeys = keymap.of([
  {
    key: "Mod-b",
    run: (view) => {
      void runVisualCommand(view, "bold");
      return true;
    },
  },
  {
    key: "Mod-i",
    run: (view) => {
      void runVisualCommand(view, "italic");
      return true;
    },
  },
]);

/** The extension for one document, bound to the host that serves it. */
export function visualEditor(host: VisualHost): Extension {
  const { field, extension } = visualField(host);
  const sync = createSync(host, field);
  return [
    extension,
    sync.plugin,
    EditorView.editorAttributes.of({ class: "cm-visual" }),
    visualTheme,
    Prec.high(visualKeys),
  ];
}
