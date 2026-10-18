/**
 * Formulas, colours and the other polish items, in the pure modules: the
 * scene migration, the operations (mirrors of the Rust ones, held in step by
 * `parity.test.ts`), the canvas primitives and their export.
 */

import { describe, expect, it } from "vitest";

import { approxMath, approxMathError } from "./mathApprox";
import { errorPlace, mathBox, mathKey, parseMathError, svgSize } from "./mathRender";
import { applyOp } from "./ops";
import { nodeVisual, renderSceneSvg } from "./render";
import {
  createScene,
  languageOf,
  migrateScene,
  type FigureNode,
  type FigureScene,
} from "./scene";
import { MIN_LABEL_SCALE, fitLabel, measure } from "./text";
import { fitRect, revealRect, visibleRect } from "./viewport";

describe("reading old formulas and colours", () => {
  it("turns tex into a LaTeX source in the same box", () => {
    const scene = migrateScene({
      version: 2,
      width: 100,
      height: 100,
      nodes: [{ kind: "math", id: "n1", x: 10, y: 50, tex: "E = mc^2", size: 20 }],
    });
    const m = scene.nodes[0] as FigureNode & { kind: "math" };
    expect(m).toMatchObject({ source: "E = mc^2", language: "latex", y: 30 });
    expect("tex" in m).toBe(false);
    expect(languageOf(m)).toBe("latex");
    const v1 = migrateScene({
      version: 1,
      width: 100,
      height: 100,
      nodes: [{ kind: "math", id: "n1", x: 1, y: 40, tex: "x", size: 16 }],
    });
    expect(v1.nodes[0]).toMatchObject({ source: "x", language: "latex", y: 24 });
  });

  it("writes hex as lowercase #rrggbb and refuses a malformed one", () => {
    const scene = migrateScene({
      version: 2,
      width: 10,
      height: 10,
      background: "#FAFAFA",
      nodes: [
        {
          kind: "rect",
          id: "n1",
          x: 0,
          y: 0,
          width: 5,
          height: 5,
          style: { stroke: "#ABC", fill: "ink", labelColor: "#0A0B0C" },
        },
      ],
    });
    expect(scene.background).toBe("#fafafa");
    expect(scene.nodes[0]?.style).toEqual({
      stroke: "#aabbcc",
      fill: "ink",
      labelColor: "#0a0b0c",
    });
    const bad = {
      version: 2,
      width: 10,
      height: 10,
      nodes: [
        {
          kind: "rect",
          id: "n1",
          x: 0,
          y: 0,
          width: 5,
          height: 5,
          style: { stroke: "#12345" },
        },
      ],
    };
    expect(() => migrateScene(bad)).toThrow(/#12345/);
    expect(() =>
      migrateScene({
        version: 2,
        width: 10,
        height: 10,
        background: "#zzz",
        nodes: [],
      }),
    ).toThrow();
  });
});

describe("operations on formulas, colours and the page", () => {
  const base: FigureScene = {
    ...createScene(960, 540),
    nodes: [
      { kind: "rect", id: "a", x: 0, y: 0, width: 100, height: 50 },
      {
        kind: "math",
        id: "m",
        x: 20,
        y: 300,
        source: "x^2",
        size: 20,
        width: 40,
        height: 28,
      },
    ],
  };

  it("scales a formula's size and measured box by whichever side moved more", () => {
    const next = applyOp(base, {
      op: "resize",
      id: "m",
      x: 30,
      y: 310,
      width: 80,
      height: 56,
    }).scene;
    expect(next.nodes[1]).toMatchObject({
      x: 30,
      y: 310,
      size: 40,
      width: 80,
      height: 56,
    });
    // Only the width moves: the formula still scales as a whole.
    const wide = applyOp(base, {
      op: "resize",
      id: "m",
      x: 20,
      y: 300,
      width: 120,
      height: 28,
    }).scene;
    expect(wide.nodes[1]).toMatchObject({ size: 60, width: 120, height: 84 });
  });

  it("stores custom colours as lowercase hex, text colour apart from stroke", () => {
    const next = applyOp(base, {
      op: "set-style",
      ids: ["a", "m"],
      style: { stroke: "#ABCDEF", labelColor: "#123456", fillOpacity: 30 },
    }).scene;
    expect(next.nodes[0]?.style).toEqual({
      stroke: "#abcdef",
      fillOpacity: 30,
      labelColor: "#123456",
    });
    expect(() =>
      applyOp(base, { op: "set-style", ids: ["a"], style: { stroke: "#12" } }),
    ).toThrow(/unknown colour/);
    expect(() =>
      applyOp(base, { op: "set-style", ids: ["a"], style: { fillOpacity: 120 } }),
    ).toThrow(/fillOpacity/);
    // Clearing the label colour makes it follow the stroke again.
    const cleared = applyOp(next, {
      op: "update",
      id: "a",
      set: { style: { labelColor: null } },
    }).scene;
    expect(cleared.nodes[0]?.style).toEqual({ stroke: "#abcdef", fillOpacity: 30 });
  });

  it("sets and clears the page background in one step", () => {
    const set = applyOp(base, {
      op: "set-canvas",
      width: 960,
      height: 540,
      background: "#FFFFFF",
    });
    expect(set.applied).toBe(true);
    expect(set.scene.background).toBe("#ffffff");
    const same = applyOp(set.scene, {
      op: "set-canvas",
      width: 960,
      height: 540,
      background: "#ffffff",
    });
    expect(same.applied).toBe(false);
    const cleared = applyOp(set.scene, {
      op: "set-canvas",
      width: 960,
      height: 540,
      background: "none",
    });
    expect(cleared.scene.background).toBeUndefined();
    expect("background" in cleared.scene).toBe(false);
    expect(() =>
      applyOp(base, { op: "set-canvas", width: 1, height: 1, background: "red" }),
    ).toThrow(/background/);
  });

  it("lets arrows bind to a formula", () => {
    const next = applyOp(
      {
        ...base,
        nodes: [
          ...base.nodes,
          {
            kind: "arrow",
            id: "e",
            points: [
              { x: 0, y: 0 },
              { x: 1, y: 1 },
            ],
          },
        ],
      },
      { op: "bind", id: "e", end: "end", node: "m" },
    ).scene;
    const e = next.nodes[2];
    expect(e?.kind === "arrow" && e.end).toEqual({ node: "m" });
  });

  it("omits Typst (the default language) and an unmeasured box", () => {
    const next = applyOp(base, {
      op: "update",
      id: "m",
      set: { language: "typst", width: 0, height: 0 },
    }).scene;
    expect("language" in (next.nodes[1] as object)).toBe(false);
    expect("width" in (next.nodes[1] as object)).toBe(false);
    expect(mathBox(next.nodes[1] as never)).toEqual({
      width: Math.max(measure("x^2", 20, "mono"), 20),
      height: 28,
    });
  });
});

describe("formula rendering helpers", () => {
  it("keys renderings by language, source, size and colour only", () => {
    const k = mathKey("typst", "x", 24, "#112233");
    expect(k).not.toBe(mathKey("latex", "x", 24, "#112233"));
    expect(k).not.toBe(mathKey("typst", "x ", 24, "#112233"));
    expect(k).not.toBe(mathKey("typst", "x", 25, "#112233"));
    expect(k).not.toBe(mathKey("typst", "x", 24, "#445566"));
    expect(k).toBe(mathKey("typst", "x", 24, "#112233"));
  });

  it("reads a rendering's size from its viewBox", () => {
    expect(
      svgSize('<svg xmlns="x" viewBox="0 0 143.4 63.7" width="143.4"><g/></svg>'),
    ).toEqual({
      width: 143.4,
      height: 63.7,
    });
    expect(svgSize("<svg></svg>")).toBeUndefined();
    expect(svgSize('<svg viewBox="0 0 0 5"></svg>')).toBeUndefined();
  });

  it("reads the structured error, or keeps the message", () => {
    const e = parseMathError(
      'Failed: [math_error] {"message":"unknown variable","line":2,"column":5,"length":3}',
    );
    expect(e).toMatchObject({ message: "unknown variable", line: 2, column: 5 });
    expect(errorPlace(e)).toBe("line 2, column 5");
    expect(parseMathError("plain failure")).toEqual({ message: "plain failure" });
    expect(parseMathError("[math_error] {broken").message).toContain("math_error");
    expect(errorPlace({ message: "x" })).toBe("");
  });

  it("the browser mock approximates, and reports broken source with a place", () => {
    const m = approxMath("latex", "x^{2}", 20);
    expect(m.svg).toContain("approximate latex math");
    expect(m.svg).toContain("currentColor");
    expect(svgSize(m.svg)).toEqual({ width: m.width, height: m.height });
    expect(approxMathError("latex", "x^{2")).toMatchObject({ line: 1, column: 3 });
    expect(approxMathError("typst", "a +\n(b")).toMatchObject({ line: 2, column: 1 });
    expect(approxMathError("typst", "a + (b)")).toBeUndefined();
    expect(() => approxMath("typst", "frac(1", 20)).toThrow();
    expect(approxMathError("typst", "  ")).toMatchObject({
      message: expect.any(String),
    });
  });
});

describe("the canvas draws what the export writes", () => {
  const node = (n: FigureNode) => nodeVisual(createScene(1, 1), n, "print");

  it("a formula is one math primitive in the stroke colour, at its box", () => {
    const v = node({
      kind: "math",
      id: "m",
      x: 10,
      y: 20,
      source: "x",
      language: "latex",
      size: 24,
      width: 30,
      height: 34,
      style: { stroke: "#ABCDEF" },
    });
    expect(v?.items).toEqual([
      {
        type: "math",
        id: "m",
        x: 10,
        y: 20,
        width: 30,
        height: 34,
        language: "latex",
        source: "x",
        size: 24,
        colour: "#abcdef",
      },
    ]);
  });

  it("text colour follows the stroke unless set; fill opacity overrides the tint", () => {
    const rect = (style: object) =>
      node({
        kind: "rect",
        id: "r",
        x: 0,
        y: 0,
        width: 100,
        height: 50,
        label: "Hi",
        style,
      } as FigureNode);
    const text = (v: ReturnType<typeof rect>) =>
      v?.items.find((i) => i.type === "text") as { fill: string } | undefined;
    const stroke = { stroke: "#112233" };
    expect(text(rect(stroke))?.fill).toBe("#112233");
    expect(text(rect({ ...stroke, labelColor: "#445566" }))?.fill).toBe("#445566");
    const fillOf = (style: object) =>
      rect(style)?.items.find((i) => i.type === "path") as { fillOpacity?: number };
    expect(fillOf({ fill: "blue" }).fillOpacity).toBe(0.2);
    expect(fillOf({ fill: "blue", fillOpacity: 60 }).fillOpacity).toBe(0.6);
    expect(fillOf({ fill: "#102030", fillOpacity: 40 }).fillOpacity).toBe(0.4);
    expect(fillOf({ fill: "none" }).fillOpacity).toBeUndefined();
  });

  it("the export draws the page background behind everything", () => {
    const scene: FigureScene = { ...createScene(100, 80), background: "paper" };
    const svg = renderSceneSvg(scene, "print");
    expect(svg).toContain('<rect x="0" y="0" width="100" height="80" fill="#ffffff"/>');
    expect(renderSceneSvg(createScene(100, 80))).not.toContain("<rect");
    const custom = renderSceneSvg({ ...scene, background: "#102030" });
    expect(custom).toContain('fill="#102030"/>');
  });
});

describe("diamond labels", () => {
  it("break only at spaces, shrinking the font before breaking a word", () => {
    const fit = fitLabel("Validate input", 50, 100, 16, "sans");
    expect(fit.lines).toEqual(["Validate", "input"]);
    expect(fit.size).toBe(13.5);
    const roomy = fitLabel("Is it ok", 80, 100, 16, "sans");
    expect(roomy.size).toBe(16);
    const long = fitLabel("Supercalifragilistic", 40, 100, 16, "sans");
    expect(long.size).toBe(16 * MIN_LABEL_SCALE);
    expect(long.lines.length).toBeGreaterThan(1);
    expect(fitLabel("", 60, 40, 16, "sans")).toEqual({ lines: [""], size: 16 });
  });

  it("the diamond's text box is about 60% of its width, and the label stays inside", () => {
    const v = node("Validate input");
    const text = v?.items.find((i) => i.type === "text") as unknown as {
      lines: { text: string }[];
      size: number;
    };
    expect(text.lines.map((l) => l.text)).toEqual(["Validate", "input"]);
    expect(text.size).toBeLessThanOrEqual(16);

    function node(label: string) {
      return nodeVisual(
        createScene(1, 1),
        { kind: "diamond", id: "d", x: 0, y: 0, width: 100, height: 80, label },
        "print",
      );
    }
  });
});

describe("revealing a rectangle", () => {
  const v = { x: 0, y: 0, zoom: 1 };

  it("leaves the view alone when the rectangle is in sight", () => {
    expect(revealRect(v, { x: 100, y: 100, width: 200, height: 100 }, 800, 600)).toBe(
      v,
    );
  });

  it("centres it at the same zoom when it fits, fits it when it does not", () => {
    const far = { x: 2000, y: 1000, width: 300, height: 200 };
    const centred = revealRect(v, far, 800, 600);
    expect(centred.zoom).toBe(1);
    expect(visibleRect(centred, 800, 600).x).toBeCloseTo(2150 - 400);
    const huge = { x: 0, y: 0, width: 4000, height: 3000 };
    const fitted = revealRect(v, huge, 800, 600);
    expect(fitted.zoom).toBeLessThan(0.3);
    expect(fitted).toEqual(fitRect(huge, 800, 600, 32, 1));
    expect(revealRect(v, far, 0, 0)).toBe(v);
  });
});
