/**
 * The writing tools as one CodeMirror extension: spell check, prose lint,
 * completions (symbols, snippets), word count and Format document.
 *
 * Everything heavy runs in the Rust `writing` plugin; this layer decides what
 * to ask for and when (idle, visible paragraphs only, cached) and draws the
 * answers. See `checker.ts` for the cadence.
 */

import { Prec, type Extension } from "@codemirror/state";
import { keymap, type EditorView } from "@codemirror/view";

import { kernel } from "$lib/kernel";
import { errorMessage } from "$lib/kernel/client";
import { writingApi, type WritingApi, type WritingSyntax } from "$lib/kernel/api";

import { emitFormat, type FormatReport } from "./bus";
import { writingCompletions } from "./complete";
import { formatView } from "./format";
import { lint, lintInvalidate, lintTheme } from "./lint";
import { spell, spellInvalidate, spellTheme } from "./spell";
import { stats } from "./stats";

let defaultApi: WritingApi | undefined;

function api(): WritingApi {
  defaultApi ??= writingApi(kernel);
  return defaultApi;
}

/** Format the document in `view` and report the outcome on the writing bus. */
export async function runFormat(
  view: EditorView,
  using: WritingApi = api(),
  syntax?: WritingSyntax,
): Promise<FormatReport> {
  let report: FormatReport;
  try {
    const outcome = await formatView(view, using, syntax);
    switch (outcome.kind) {
      case "formatted":
        report = { kind: "formatted", message: "Document formatted." };
        break;
      case "unchanged":
        report = { kind: "unchanged", message: "Already formatted." };
        break;
      case "stale":
        report = {
          kind: "stale",
          message: "Not formatted: the text changed meanwhile.",
        };
        break;
      default:
        report = { kind: outcome.kind, message: outcome.message };
    }
  } catch (error) {
    report = { kind: "failed", message: `Could not format: ${errorMessage(error)}` };
  }
  emitFormat(view, report);
  return report;
}

/** Drop cached answers and check the visible text again. */
export function recheckWriting(view: EditorView): void {
  view.dispatch({ effects: [spellInvalidate.of(null), lintInvalidate.of(null)] });
}

export function writing(using: WritingApi = api()): Extension {
  return [
    spell(using),
    spellTheme,
    lint(using),
    lintTheme,
    writingCompletions(using),
    stats(using),
    Prec.high(
      keymap.of([
        {
          key: "Shift-Alt-f",
          preventDefault: true,
          run: (view) => {
            void runFormat(view, using);
            return true;
          },
        },
      ]),
    ),
  ];
}

export { subscribeWriting } from "./bus";
