/**
 * Margin Whispers — the gutter and hover callout from DESIGN.md §3.2.
 *
 * A diagnostic never raises a modal or prints a terminal dump. It becomes a 4px
 * square in the line-number gutter; hovering reveals one quiet line, and when a
 * fix exists, one click applies it.
 *
 * Diagnostics live in a state field rather than being passed per decoration so
 * the gutter, the hover callout and the tests all read one list. Positions are
 * remapped through every transaction, otherwise the whispers would drift off
 * their lines as soon as the author types above them.
 */

import { StateEffect, StateField, type Extension } from "@codemirror/state";
import {
  GutterMarker,
  gutter,
  hoverTooltip,
  type EditorView,
  type Tooltip,
} from "@codemirror/view";

import { applyFix, type Diagnostic } from "./diagnostics";

/** A whisper on one line. */
class WhisperMarker extends GutterMarker {
  constructor(private readonly severity: Diagnostic["severity"]) {
    super();
  }

  override toDOM(): Node {
    const dot = document.createElement("span");
    // 4px square, per the spec — never a circle.
    dot.className = "manang-whisper";
    // Errors read in the maths token, warnings in secondary text: severity is
    // never signalled by hue alone, and the label beside it carries the meaning.
    dot.style.backgroundColor =
      this.severity === "error" ? "var(--mng-math)" : "var(--mng-text-60)";
    return dot;
  }
}

/** Keeps the gutter the same width whether or not a line has a whisper. */
class WhisperSpacer extends GutterMarker {
  override toDOM(): Node {
    const spacer = document.createElement("span");
    spacer.className = "manang-whisper";
    spacer.style.visibility = "hidden";
    return spacer;
  }
}

/** Replaces the live diagnostic list. */
export const setDiagnostics = StateEffect.define<readonly Diagnostic[]>();

const diagnosticField = StateField.define<readonly Diagnostic[]>({
  create() {
    return [];
  },
  update(value, transaction) {
    for (const effect of transaction.effects) {
      if (effect.is(setDiagnostics)) return effect.value;
    }

    if (!transaction.docChanged) return value;

    // Map both ends forward. `from` uses -1 (stick to the character before) so a
    // whisper stays on its own line rather than sliding to the previous one.
    return value
      .map((diagnostic) => {
        const from = transaction.changes.mapPos(diagnostic.from, -1);
        const to = transaction.changes.mapPos(diagnostic.to, 1);
        return to <= from ? undefined : { ...diagnostic, from, to };
      })
      .filter((entry): entry is Diagnostic => entry !== undefined);
  },
});

/** The whisper gutter: 4px squares beside the line numbers. */
export function whisperGutter(): Extension {
  return gutter({
    class: "manang-whisper-gutter",
    lineMarker(view, line) {
      const list = view.state.field(diagnosticField, false) ?? [];
      const hit = list.find(
        (diagnostic) => diagnostic.from <= line.to && diagnostic.to >= line.from,
      );
      return hit === undefined ? null : new WhisperMarker(hit.severity);
    },
    lineMarkerChange(update) {
      return update.docChanged || update.viewportChanged;
    },
    initialSpacer: () => new WhisperSpacer(),
  });
}

/** The hover callout, including the one-click fix where one exists. */
export function whisperCallout(onFix: (next: string) => void): Extension {
  return hoverTooltip((view, position): Tooltip | null => {
    const list = view.state.field(diagnosticField, false) ?? [];
    const hits = list.filter(
      (diagnostic) => position >= diagnostic.from && position <= diagnostic.to,
    );

    const primary = hits[0];
    if (primary === undefined) return null;

    return {
      pos: primary.from,
      above: true,
      create: () => {
        const wrapper = document.createElement("div");
        wrapper.className = "manang-whisper-tooltip";

        for (const diagnostic of hits) {
          const row = document.createElement("div");
          row.className = "manang-whisper-row";

          const label = document.createElement("span");
          label.className = "manang-whisper-label";
          label.textContent = diagnostic.severity;
          row.append(label);

          const message = document.createElement("span");
          message.className = "manang-whisper-message";
          message.textContent = diagnostic.message;
          row.append(message);

          if (diagnostic.fix !== undefined) {
            const fix = document.createElement("button");
            fix.type = "button";
            fix.className = "manang-whisper-fix";
            fix.textContent = `Auto-Fix · ${diagnostic.fix.title}`;
            fix.addEventListener("mousedown", (event) => {
              // Keep the editor selection: a click that blurs first would lose
              // the caret the author is working from.
              event.preventDefault();
            });
            fix.addEventListener("click", (event) => {
              event.preventDefault();
              const next = applyFix(view.state.doc.toString(), diagnostic);
              if (next === null) return;
              view.dispatch({
                changes: { from: 0, to: view.state.doc.length, insert: next },
              });
              onFix(next);
            });
            row.append(fix);
          }

          wrapper.append(row);
        }

        return { dom: wrapper };
      },
    };
  });
}

/** Gutter plus callout, wired to the editor state. */
export function whispers(onFix: (next: string) => void): Extension[] {
  return [diagnosticField, whisperGutter(), whisperCallout(onFix)];
}

/** Replaces the diagnostics held by an editor. */
export function pushDiagnostics(
  view: EditorView,
  diagnostics: readonly Diagnostic[],
): void {
  view.dispatch({ effects: setDiagnostics.of(diagnostics) });
}

/** Reads the diagnostics currently held by an editor. */
export function readDiagnostics(view: EditorView): readonly Diagnostic[] {
  return view.state.field(diagnosticField, false) ?? [];
}
