/**
 * Prose lint: style advice underlined in the text and listed under Problems.
 *
 * Same cadence and cache as spelling. The underline is dotted for advice and
 * dashed for warnings, a different shape from the spelling squiggle and from
 * the gutter squares compile errors use, so the three never read alike.
 */

import { StateEffect, StateField, type Extension } from "@codemirror/state";
import {
  Decoration,
  EditorView,
  hoverTooltip,
  type DecorationSet,
  type Tooltip,
} from "@codemirror/view";

import type { WritingApi, WritingDiagnostic } from "$lib/kernel/api";

import { emitLint, type FindingSeverity, type LintFinding } from "./bus";
import { checker, newInvalidation } from "./checker";
import type { RelItem } from "./chunks";

export const lintInvalidate = newInvalidation();

interface LintItem extends RelItem {
  readonly severity: FindingSeverity;
  readonly message: string;
  readonly code: string;
  readonly hints: readonly string[];
}

const setLint = StateEffect.define<readonly LintFinding[]>();

function markFor(finding: LintFinding): Decoration {
  return Decoration.mark({
    class: `cm-prose cm-prose-${finding.severity}`,
    finding,
  });
}

export const lintField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(setLint)) {
        return Decoration.set(
          effect.value.map((f) => markFor(f).range(f.from, f.to)),
          true,
        );
      }
    }
    return tr.docChanged ? value.map(tr.changes) : value;
  },
  provide: (field) => EditorView.decorations.from(field),
});

function lintHover(): Extension {
  return hoverTooltip((view, pos): Tooltip | null => {
    const hits: LintFinding[] = [];
    view.state.field(lintField, false)?.between(pos, pos, (from, to, value) => {
      const finding = (value.spec as { finding?: LintFinding }).finding;
      if (finding !== undefined && from <= pos && pos <= to) hits.push(finding);
    });
    const first = hits[0];
    if (first === undefined) return null;
    return {
      pos: first.from,
      above: true,
      create: () => {
        const dom = document.createElement("div");
        dom.className = "manang-problem-tooltip";
        for (const finding of hits) {
          const row = document.createElement("div");
          row.className = "manang-problem-row";
          const label = document.createElement("span");
          label.className = "manang-problem-label";
          label.textContent = finding.severity === "info" ? "Advice" : "Writing";
          const message = document.createElement("span");
          message.className = "manang-problem-message";
          message.textContent = finding.message;
          row.append(label, message);
          dom.append(row);
        }
        return { dom };
      },
    };
  });
}

function severityOf(d: WritingDiagnostic): FindingSeverity {
  return d.severity;
}

export function lint(api: WritingApi): Extension {
  return [
    lintField,
    lintHover(),
    checker<WritingDiagnostic, LintItem>(
      {
        request: async (text, syntax) => (await api.lint(text, syntax)).diagnostics,
        range: (d) => d.range,
        make: (d, start, end) => ({
          start,
          end,
          severity: severityOf(d),
          message: d.message,
          code: d.code ?? "",
          hints: d.hints,
        }),
        apply: (view, chunks, found) => {
          const findings: LintFinding[] = [];
          chunks.forEach((chunk, i) => {
            for (const item of found[i] ?? []) {
              findings.push({
                from: chunk.from + item.start,
                to: chunk.from + item.end,
                severity: item.severity,
                message: item.message,
                code: item.code,
                hints: item.hints,
              });
            }
          });
          view.dispatch({ effects: setLint.of(findings) });
          emitLint(view, findings);
        },
      },
      lintInvalidate,
    ),
  ];
}

export const lintTheme = EditorView.baseTheme({
  ".cm-prose": {
    textDecorationLine: "underline",
    textDecorationThickness: "1px",
    textUnderlineOffset: "3px",
    textDecorationColor: "var(--mng-text-muted)",
  },
  ".cm-prose-info": { textDecorationStyle: "dotted" },
  ".cm-prose-warning": { textDecorationStyle: "dashed" },
  ".cm-prose-error": { textDecorationStyle: "solid" },
});
