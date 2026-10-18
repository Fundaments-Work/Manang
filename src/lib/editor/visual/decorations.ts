/**
 * Decorations from a span table.
 *
 * Pure with respect to the DOM: it only builds `Decoration` values (widgets
 * create their elements later, when the view draws them), so the rules are
 * unit-tested in node. The document is never touched; everything here is a way
 * of looking at it.
 *
 * Output is two sets: `decorations` (what is drawn) and `atomic` (the ranges a
 * caret must not land inside: hidden markers and replaced constructs).
 */

import { RangeSet, type ChangeDesc, type Range, type Text } from "@codemirror/state";
import { Decoration, type DecorationSet } from "@codemirror/view";

import type { VisualHost } from "./host";
import { LIST_AUX, MATH_AUX, SPAN_KIND, type Span } from "./protocol";
import { isLeaf, markerRevealed, sourceRevealed, type Sel } from "./reveal";
import {
  FrontmatterWidget,
  ImageWidget,
  ListMarkerWidget,
  MathWidget,
  RuleWidget,
} from "./widgets";

export interface Built {
  readonly decorations: DecorationSet;
  readonly atomic: DecorationSet;
}

export const EMPTY_BUILT: Built = {
  decorations: Decoration.none,
  atomic: Decoration.none,
};

const HIDDEN = Decoration.replace({});
const lineClass = new Map<string, Decoration>();
const markClass = new Map<string, Decoration>();

function line(cls: string): Decoration {
  let d = lineClass.get(cls);
  if (d === undefined) {
    d = Decoration.line({ class: cls });
    lineClass.set(cls, d);
  }
  return d;
}

function mark(cls: string): Decoration {
  let d = markClass.get(cls);
  if (d === undefined) {
    d = Decoration.mark({ class: cls });
    markClass.set(cls, d);
  }
  return d;
}

const INLINE_CLASS: ReadonlyMap<number, string> = new Map([
  [SPAN_KIND.strong, "cm-vis-strong"],
  [SPAN_KIND.emph, "cm-vis-emph"],
  [SPAN_KIND.code, "cm-vis-code"],
  [SPAN_KIND.strike, "cm-vis-strike"],
  [SPAN_KIND.opaque, "cm-vis-opaque"],
  [SPAN_KIND.comment, "cm-vis-comment"],
  [SPAN_KIND.ref, "cm-vis-ref"],
  [SPAN_KIND.label, "cm-vis-ref"],
  [SPAN_KIND.cite, "cm-vis-ref"],
  [SPAN_KIND.table, "cm-vis-opaque"],
]);

/** The spans of a table moved through changes made since it was produced. */
export function mapSpans(spans: readonly Span[], changes: ChangeDesc | null): Span[] {
  if (changes === null || changes.empty) return spans.slice();
  const out: Span[] = [];
  for (const span of spans) {
    // The start stays right of text typed at it, the end left of text typed at it.
    const start = changes.mapPos(span.start, 1);
    const end = changes.mapPos(span.end, -1);
    if (end > start) out.push({ ...span, start, end });
  }
  return out;
}

/** The text of a formula without its `$` delimiters and padding. */
export function mathSource(text: string): string {
  return text.replace(/^\$+\s*/, "").replace(/\s*\$+$/, "");
}

/** The alt text of a Markdown image `![alt](src)`, or "". */
export function imageAlt(text: string): string {
  return /^!\[([^\]]*)\]/.exec(text)?.[1] ?? "";
}

function eachLine(
  doc: Text,
  from: number,
  to: number,
  fn: (lineFrom: number) => void,
): void {
  let at = from;
  for (;;) {
    const l = doc.lineAt(at);
    fn(l.from);
    if (l.to >= to || l.to >= doc.length) return;
    at = l.to + 1;
  }
}

/**
 * Build the decorations for `spans` (positions in `doc`) given the selection.
 * Spans outside the document are ignored, so a stale table can never throw.
 */
export function buildDecorations(
  spans: readonly Span[],
  doc: Text,
  selection: readonly Sel[],
  host: VisualHost,
): Built {
  const deco: Range<Decoration>[] = [];
  const atomic: Range<Decoration>[] = [];
  const stack: Span[] = [];
  /** Everything before this position is inside a replaced construct. */
  let replacedUntil = 0;
  const hide = (from: number, to: number): void => {
    deco.push(HIDDEN.range(from, to));
    atomic.push(HIDDEN.range(from, to));
  };
  const widget = (from: number, to: number, w: Decoration): void => {
    deco.push(w.range(from, to));
    atomic.push(HIDDEN.range(from, to));
    replacedUntil = to;
  };

  for (const span of spans) {
    if (span.end > doc.length || span.start >= span.end) continue;
    while (stack.length > 0 && (stack[stack.length - 1]?.end ?? 0) <= span.start) {
      stack.pop();
    }
    const parent = stack[stack.length - 1];
    const kind = span.kind as number;
    const insideLeaf = stack.some((s) => isLeaf(s.kind as number));
    stack.push(span);
    if (span.start < replacedUntil) continue;

    if (kind === SPAN_KIND.marker) {
      if (insideLeaf && parent !== undefined && !isLeaf(parent.kind as number))
        continue;
      const revealed = markerRevealed(doc, selection, parent, span);
      if (parent?.kind === SPAN_KIND.listItem) {
        listMarker(span, parent, revealed, deco, widget);
      } else if (revealed) {
        deco.push(mark("cm-vis-marker").range(span.start, span.end));
      } else {
        hide(span.start, span.end);
      }
      continue;
    }
    if (insideLeaf) continue;

    switch (kind) {
      case SPAN_KIND.heading: {
        const cls = `cm-vis-h cm-vis-h${Math.min(6, Math.max(1, span.aux))}`;
        eachLine(doc, span.start, span.end, (at) => deco.push(line(cls).range(at)));
        break;
      }
      case SPAN_KIND.quote:
        eachLine(doc, span.start, span.end, (at) =>
          deco.push(line("cm-vis-quote").range(at)),
        );
        break;
      case SPAN_KIND.listItem:
        deco.push(
          line(
            span.aux === LIST_AUX.ordered ? "cm-vis-li cm-vis-li-ordered" : "cm-vis-li",
          ).range(doc.lineAt(span.start).from),
        );
        break;
      case SPAN_KIND.codeBlock:
        eachLine(doc, span.start, span.end, (at) =>
          deco.push(line("cm-vis-codeblock").range(at)),
        );
        break;
      case SPAN_KIND.link:
        deco.push(mark("cm-vis-link").range(span.start, span.end));
        break;
      case SPAN_KIND.frontmatter:
        if (sourceRevealed(doc, selection, span)) {
          eachLine(doc, span.start, span.end, (at) =>
            deco.push(line("cm-vis-frontmatter-source").range(at)),
          );
        } else {
          const lines = doc.lineAt(span.end).number - doc.lineAt(span.start).number + 1;
          widget(
            span.start,
            span.end,
            Decoration.replace({ widget: new FrontmatterWidget(lines) }),
          );
        }
        break;
      case SPAN_KIND.rule:
        if (!sourceRevealed(doc, selection, span)) {
          widget(
            span.start,
            span.end,
            Decoration.replace({ widget: new RuleWidget() }),
          );
        } else deco.push(mark("cm-vis-source").range(span.start, span.end));
        break;
      case SPAN_KIND.image:
        if (!sourceRevealed(doc, selection, span)) {
          const text = doc.sliceString(span.start, span.end);
          widget(
            span.start,
            span.end,
            Decoration.replace({
              widget: new ImageWidget(
                host,
                span.attr ?? "",
                imageAlt(text),
                span.aux === 1,
              ),
            }),
          );
        } else deco.push(mark("cm-vis-source").range(span.start, span.end));
        break;
      case SPAN_KIND.math:
        if (!sourceRevealed(doc, selection, span)) {
          const source = mathSource(doc.sliceString(span.start, span.end));
          widget(
            span.start,
            span.end,
            Decoration.replace({
              widget: new MathWidget(
                host,
                span.attr === "latex" ? "latex" : "typst",
                source,
                span.aux === MATH_AUX.block,
              ),
            }),
          );
        } else deco.push(mark("cm-vis-source").range(span.start, span.end));
        break;
      default: {
        const cls = INLINE_CLASS.get(kind);
        if (cls !== undefined) deco.push(mark(cls).range(span.start, span.end));
        // An adapter's own kind: shown as an opaque chip of source.
        else if (kind >= 1024)
          deco.push(mark("cm-vis-opaque").range(span.start, span.end));
      }
    }
  }
  return {
    decorations: RangeSet.of(deco, true),
    atomic: RangeSet.of(atomic, true),
  };
}

function listMarker(
  span: Span,
  parent: Span,
  revealed: boolean,
  deco: Range<Decoration>[],
  widget: (from: number, to: number, w: Decoration) => void,
): void {
  if (revealed) {
    deco.push(mark("cm-vis-marker").range(span.start, span.end));
    return;
  }
  switch (parent.aux) {
    case LIST_AUX.ordered:
      deco.push(mark("cm-vis-num").range(span.start, span.end));
      return;
    case LIST_AUX.taskOpen:
    case LIST_AUX.taskDone:
      widget(
        span.start,
        span.end,
        Decoration.replace({
          widget: new ListMarkerWidget(
            parent.aux === LIST_AUX.taskDone ? "done" : "open",
          ),
        }),
      );
      return;
    default:
      widget(
        span.start,
        span.end,
        Decoration.replace({ widget: new ListMarkerWidget(null) }),
      );
  }
}
