/**
 * Problem markers: the gutter squares and hover callout.
 *
 * A diagnostic never raises a modal or prints a terminal dump. It becomes a
 * small square beside the line number: solid for an error, hollow for a
 * warning, so severity is carried by shape and never by hue. Hovering reveals
 * the message and the compiler's hints. The Problems panel lists the same set.
 *
 * Diagnostics live in a state field so the gutter, the callout and the tests
 * read one list. Positions are remapped through every transaction, otherwise
 * the markers would drift off their lines as soon as the author types above.
 */

import { StateEffect, StateField, type Extension } from "@codemirror/state";
import {
  GutterMarker,
  gutter,
  hoverTooltip,
  type EditorView,
  type Tooltip,
} from "@codemirror/view";

import type { Diagnostic } from "./diagnostics";

/** A marker on one line. */
class ProblemMarker extends GutterMarker {
  constructor(private readonly severity: Diagnostic["severity"]) {
    super();
  }

  override eq(other: GutterMarker): boolean {
    return other instanceof ProblemMarker && other.severity === this.severity;
  }

  override toDOM(): Node {
    const mark = document.createElement("span");
    mark.className = `manang-problem manang-problem-${this.severity}`;
    return mark;
  }
}

/** Keeps the gutter the same width whether or not a line has a marker. */
class ProblemSpacer extends GutterMarker {
  override toDOM(): Node {
    const spacer = document.createElement("span");
    spacer.className = "manang-problem";
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

    // `from` sticks to the character before so a marker stays on its own line.
    return value
      .map((diagnostic) => {
        const from = transaction.changes.mapPos(diagnostic.from, -1);
        const to = transaction.changes.mapPos(diagnostic.to, 1);
        return to <= from ? undefined : { ...diagnostic, from, to };
      })
      .filter((entry): entry is Diagnostic => entry !== undefined);
  },
});

export function problemGutter(): Extension {
  return gutter({
    class: "manang-problem-gutter",
    lineMarker(view, line) {
      const list = view.state.field(diagnosticField, false) ?? [];
      const hits = list.filter(
        (diagnostic) => diagnostic.from <= line.to && diagnostic.to >= line.from,
      );
      if (hits.length === 0) return null;
      // An error on the line outranks a warning.
      const severity = hits.some((h) => h.severity === "error") ? "error" : "warning";
      return new ProblemMarker(severity);
    },
    lineMarkerChange(update) {
      // A new diagnostic list arrives as an effect, with no document change.
      return (
        update.docChanged ||
        update.viewportChanged ||
        update.transactions.some((tr) =>
          tr.effects.some((effect) => effect.is(setDiagnostics)),
        )
      );
    },
    initialSpacer: () => new ProblemSpacer(),
  });
}

export function problemCallout(): Extension {
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
        wrapper.className = "manang-problem-tooltip";

        for (const diagnostic of hits) {
          const row = document.createElement("div");
          row.className = "manang-problem-row";

          const label = document.createElement("span");
          label.className = "manang-problem-label";
          label.textContent = diagnostic.severity === "error" ? "Error" : "Warning";
          row.append(label);

          const message = document.createElement("span");
          message.className = "manang-problem-message";
          message.textContent = diagnostic.message;
          row.append(message);

          wrapper.append(row);
        }

        return { dom: wrapper };
      },
    };
  });
}

/** Gutter plus callout, wired to the editor state. */
export function problemMarkers(): Extension[] {
  return [diagnosticField, problemGutter(), problemCallout()];
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
