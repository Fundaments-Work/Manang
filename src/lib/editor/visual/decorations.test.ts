import {
  ChangeSet,
  EditorSelection,
  EditorState,
  Text,
  type TransactionSpec,
} from "@codemirror/state";
import { describe, expect, it } from "vitest";

import { spansOf, toTable } from "$lib/kernel/mockVisualSpans";

import { buildDecorations, mapSpans, mathSource, type Built } from "./decorations";
import { visualField, setTable, currentSpans, coverage } from "./field";
import type { VisualHost } from "./host";
import { SPAN_KIND, type Span } from "./protocol";
import { markerRevealed, sourceRevealed, touches } from "./reveal";
import { decodeSpanTable, encodeSpanTable, type SpanTable } from "./table";

const host: VisualHost = {
  path: "doc.md",
  api: {} as VisualHost["api"],
  readBytes: () => Promise.reject(new Error("no files")),
  renderMath: () => Promise.reject(new Error("no math")),
  resolveAsset: () => null,
  figureFor: () => null,
  openFigure: () => undefined,
};

/** Spans of `text` as the contract delivers them (decoded from the binary table). */
function tableOf(format: "markdown" | "typst", text: string): SpanTable {
  const bytes = encodeSpanTable({
    revision: 1,
    from: 0,
    to: text.length,
    docLen: text.length,
    spans: toTable(spansOf(format, text), text.length),
  });
  const result = decodeSpanTable(bytes);
  if (!result.ok) throw new Error(result.reason);
  return result.table;
}

interface Item {
  from: number;
  to: number;
  kind: "hide" | "widget" | "mark" | "line";
  cls?: string;
  widget?: string;
}

function items(built: Built): Item[] {
  const out: Item[] = [];
  built.decorations.between(0, 1e9, (from, to, value) => {
    const spec = value.spec as {
      class?: string;
      widget?: { constructor: { name: string } };
    };
    if (spec.widget !== undefined) {
      out.push({ from, to, kind: "widget", widget: spec.widget.constructor.name });
    } else if (value.point && from !== to) out.push({ from, to, kind: "hide" });
    else if (value.point) out.push({ from, to, kind: "line", cls: spec.class ?? "" });
    else out.push({ from, to, kind: "mark", cls: spec.class ?? "" });
  });
  return out;
}

function build(
  format: "markdown" | "typst",
  text: string,
  caret: number | [number, number],
): Built {
  const state = EditorState.create({ doc: text });
  const [anchor, head] = Array.isArray(caret) ? caret : [caret, caret];
  return buildDecorations(
    tableOf(format, text).spans,
    state.doc,
    [{ from: Math.min(anchor, head), to: Math.max(anchor, head) }],
    host,
  );
}

const hidden = (built: Built): [number, number][] =>
  items(built)
    .filter((i) => i.kind === "hide")
    .map((i) => [i.from, i.to]);

describe("reveal rules", () => {
  it("touches includes both ends", () => {
    expect(touches([{ from: 3, to: 3 }], 3, 6)).toBe(true);
    expect(touches([{ from: 6, to: 6 }], 3, 6)).toBe(true);
    expect(touches([{ from: 7, to: 7 }], 3, 6)).toBe(false);
    expect(
      touches(
        [
          { from: 0, to: 1 },
          { from: 5, to: 5 },
        ],
        3,
        6,
      ),
    ).toBe(true);
  });

  it("scopes list and heading markers to their line, other markers to the construct", () => {
    const doc = Text.of(["- one", "- two"]);
    const item = (start: number, end: number): Span => ({
      start,
      end,
      kind: SPAN_KIND.listItem,
      layer: 0,
      aux: 0,
      attr: null,
    });
    const marker = (start: number): Span => ({
      start,
      end: start + 2,
      kind: SPAN_KIND.marker,
      layer: 1,
      aux: 0,
      attr: null,
    });
    // One caret on the second line reveals only that line's marker.
    const caret = [{ from: 8, to: 8 }];
    expect(markerRevealed(doc, caret, item(0, 11), marker(6))).toBe(true);
    expect(markerRevealed(doc, caret, item(0, 11), marker(0))).toBe(false);
    const strong: Span = { ...item(0, 11), kind: SPAN_KIND.strong };
    expect(markerRevealed(doc, caret, strong, marker(0))).toBe(true);
    expect(markerRevealed(doc, caret, undefined, marker(0))).toBe(true);
  });

  it("reveals a rule per line", () => {
    const doc = Text.of(["a", "---", "b"]);
    const rule: Span = {
      start: 2,
      end: 5,
      kind: SPAN_KIND.rule,
      layer: 0,
      aux: 0,
      attr: null,
    };
    expect(sourceRevealed(doc, [{ from: 3, to: 3 }], rule)).toBe(true);
    expect(sourceRevealed(doc, [{ from: 0, to: 0 }], rule)).toBe(false);
  });
});

describe("decorations from a table", () => {
  it("hides heading and emphasis markers and sizes the heading line", () => {
    const text = "# Title\n\nSome **bold** text\n";
    const built = build("markdown", text, text.length);
    expect(hidden(built)).toContainEqual([0, 2]);
    expect(hidden(built)).toContainEqual([text.indexOf("**"), text.indexOf("**") + 2]);
    const list = items(built);
    expect(
      list.find((i) => i.kind === "line" && i.cls?.includes("cm-vis-h1")),
    ).toBeDefined();
    expect(
      list.find((i) => i.kind === "mark" && i.cls === "cm-vis-strong"),
    ).toMatchObject({
      from: text.indexOf("**"),
      to: text.indexOf("**", text.indexOf("**") + 2) + 2,
    });
  });

  it("reveals the markers of exactly the construct the caret touches", () => {
    const text = "# Title\n\nSome **bold** and *it* text\n";
    const inBold = build("markdown", text, text.indexOf("bold") + 2);
    const marks = hidden(inBold);
    expect(marks).not.toContainEqual([text.indexOf("**"), text.indexOf("**") + 2]);
    // The heading and the italic markers stay hidden.
    expect(marks).toContainEqual([0, 2]);
    expect(marks).toContainEqual([text.indexOf("*it"), text.indexOf("*it") + 1]);
    const onHeading = build("markdown", text, 3);
    expect(hidden(onHeading)).not.toContainEqual([0, 2]);
    expect(
      items(onHeading).some(
        (i) => i.kind === "mark" && i.cls === "cm-vis-marker" && i.from === 0,
      ),
    ).toBe(true);
  });

  it("makes hidden markers atomic and nothing else", () => {
    const text = "Some **bold** text";
    const built = build("markdown", text, 0);
    const ranges: [number, number][] = [];
    built.atomic.between(0, 1e9, (from, to) => void ranges.push([from, to]));
    expect(ranges).toEqual([
      [5, 7],
      [11, 13],
    ]);
  });

  it("replaces a list marker with a bullet and an ordered one is kept as a number", () => {
    const text = "- one\n- two\n\n1. first\n";
    const built = build("markdown", text, text.length);
    const widgets = items(built).filter((i) => i.kind === "widget");
    expect(widgets.map((w) => w.widget)).toEqual([
      "ListMarkerWidget",
      "ListMarkerWidget",
    ]);
    expect(items(built).some((i) => i.cls === "cm-vis-num")).toBe(true);
  });

  it("draws formulas, images, rules and front matter as widgets, and shows source at the caret", () => {
    const text =
      "---\ntitle: x\n---\n\nInline $x^2$ here\n\n![alt](fig.svg)\n\n---\n\nend\n";
    const away = items(build("markdown", text, text.length)).filter(
      (i) => i.kind === "widget",
    );
    expect(away.map((w) => w.widget).sort()).toEqual([
      "FrontmatterWidget",
      "ImageWidget",
      "MathWidget",
      "RuleWidget",
    ]);
    const mathAt = text.indexOf("$x^2$");
    const caretIn = items(build("markdown", text, mathAt + 2)).filter(
      (i) => i.kind === "widget",
    );
    expect(caretIn.map((w) => w.widget)).not.toContain("MathWidget");
    const fmIn = items(build("markdown", text, 4)).filter((i) => i.kind === "widget");
    expect(fmIn.map((w) => w.widget)).not.toContain("FrontmatterWidget");
  });

  it("never decorates inside code, math or opaque spans", () => {
    const text = "Use `**not bold**` here\n";
    const built = build("markdown", text, text.length);
    expect(items(built).some((i) => i.cls === "cm-vis-strong")).toBe(false);
    // Only the backticks are markers.
    expect(hidden(built)).toEqual([
      [4, 5],
      [17, 18],
    ]);
  });

  it("keeps Typst directives visible as styled source", () => {
    const text = '#set page(paper: "a4")\n\n= Intro\n';
    const built = build("typst", text, text.length);
    const opaque = items(built).find((i) => i.cls === "cm-vis-opaque");
    expect(opaque).toMatchObject({ from: 0, to: 22 });
    expect(hidden(built)).toContainEqual([24, 26]);
  });

  it("ignores spans outside the document and never throws on a stale table", () => {
    const span: Span = {
      start: 5,
      end: 99,
      kind: SPAN_KIND.strong,
      layer: 0,
      aux: 0,
      attr: null,
    };
    const state = EditorState.create({ doc: "short" });
    expect(() =>
      buildDecorations([span], state.doc, [{ from: 0, to: 0 }], host),
    ).not.toThrow();
    expect(
      items(buildDecorations([span], state.doc, [{ from: 0, to: 0 }], host)),
    ).toEqual([]);
  });

  it("marks an adapter-defined kind as an opaque chip", () => {
    const span: Span = {
      start: 0,
      end: 4,
      kind: 2048,
      layer: 0,
      aux: 0,
      attr: "thing",
    };
    const state = EditorState.create({ doc: "abcd" });
    const built = buildDecorations([span], state.doc, [{ from: 9, to: 9 }], host);
    expect(items(built)).toEqual([
      { from: 0, to: 4, kind: "mark", cls: "cm-vis-opaque" },
    ]);
  });
});

describe("mapSpans and helpers", () => {
  it("moves spans through changes and drops deleted ones", () => {
    const spans: Span[] = [
      { start: 2, end: 6, kind: SPAN_KIND.strong, layer: 0, aux: 0, attr: null },
      { start: 8, end: 10, kind: SPAN_KIND.emph, layer: 0, aux: 0, attr: null },
    ];
    const changes = ChangeSet.of(
      [
        { from: 0, insert: "xx" },
        { from: 7, to: 10, insert: "" },
      ],
      12,
    );
    const mapped = mapSpans(spans, changes);
    expect(mapped).toEqual([{ ...spans[0], start: 4, end: 8 }]);
    // Text typed at the start of a span stays outside it, at the end too.
    const edge = ChangeSet.of(
      [
        { from: 2, insert: "a" },
        { from: 6, insert: "b" },
      ],
      12,
    );
    expect(mapSpans([spans[0] as Span], edge)).toEqual([
      { ...spans[0], start: 3, end: 7 },
    ]);
  });

  it("strips math delimiters and padding", () => {
    expect(mathSource("$x^2$")).toBe("x^2");
    expect(mathSource("$$ a = b $$")).toBe("a = b");
    expect(mathSource("$ x $")).toBe("x");
  });
});

describe("the state field", () => {
  function setup(text: string) {
    const { field, extension } = visualField(host);
    let state = EditorState.create({ doc: text, extensions: [extension] });
    return {
      field,
      get state() {
        return state;
      },
      dispatch(spec: TransactionSpec) {
        state = state.update(spec).state;
        return state;
      },
    };
  }

  it("holds nothing until a table arrives, then decorates", () => {
    const text = "# T\n\nSome **b** end\n";
    const s = setup(text);
    expect(items(s.state.field(s.field).built)).toEqual([]);
    s.dispatch({ effects: setTable.of(tableOf("markdown", text)) });
    expect(items(s.state.field(s.field).built).length).toBeGreaterThan(0);
    s.dispatch({ effects: setTable.of(null) });
    expect(items(s.state.field(s.field).built)).toEqual([]);
  });

  it("keeps decorations attached through typing, before the next table", () => {
    const text = "Some **bold** end";
    const s = setup(text);
    s.dispatch({
      effects: setTable.of(tableOf("markdown", text)),
      selection: { anchor: 0 },
    });
    s.dispatch({ changes: { from: 0, insert: "ABC " }, selection: { anchor: 4 } });
    const value = s.state.field(s.field);
    expect(value.pending).not.toBeNull();
    const hide = items(value.built).filter((i) => i.kind === "hide");
    expect(hide.map((i) => [i.from, i.to])).toEqual([
      [9, 11],
      [15, 17],
    ]);
    expect(s.state.sliceDoc(9, 11)).toBe("**");
    // The table itself, moved to the new text, agrees.
    expect(currentSpans(value).find((x) => x.kind === SPAN_KIND.strong)).toMatchObject({
      start: 9,
      end: 17,
    });
  });

  it("rebuilds from the mapped table when only the selection moves", () => {
    const text = "Some **bold** end";
    const s = setup(text);
    s.dispatch({
      effects: setTable.of(tableOf("markdown", text)),
      selection: { anchor: 0 },
    });
    s.dispatch({ changes: { from: 0, insert: "ABC " } });
    s.dispatch({ selection: EditorSelection.single(12) });
    const hide = items(s.state.field(s.field).built).filter((i) => i.kind === "hide");
    // The caret is inside the bold text now: its markers show.
    expect(hide).toEqual([]);
  });

  it("reports the covered range in current positions", () => {
    const text = "abc def";
    const s = setup(text);
    s.dispatch({ effects: setTable.of(tableOf("markdown", text)) });
    expect(coverage(s.state.field(s.field))).toEqual({ from: 0, to: 7 });
    s.dispatch({ changes: { from: 0, insert: "zz" } });
    expect(coverage(s.state.field(s.field))).toEqual({ from: 2, to: 9 });
  });
});
