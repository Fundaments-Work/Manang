import { history, undo, redo } from "@codemirror/commands";
import { EditorState } from "@codemirror/state";
import { describe, expect, it } from "vitest";

import { planFits } from "$lib/editor/visual/client";
import { planToSpec } from "$lib/editor/visual/extension";
import type { CommandState } from "$lib/editor/visual/protocol";
import { plan } from "$lib/kernel/mockVisual";

import {
  headingArg,
  isDisabled,
  isPressed,
  itemsFor,
  relativeTo,
  tooltip,
} from "./visualToolbar";
import {
  figureSceneFor,
  opensVisual,
  resolveAsset,
  visualFormat,
  withVisualDefault,
} from "./visual";

const ALL = [
  "bold",
  "italic",
  "code",
  "strike",
  "heading",
  "list",
  "quote",
  "link",
  "image",
  "figure",
  "math",
  "rule",
].map((command): CommandState => ({ command, enabled: true, active: false }));

describe("toolbar items", () => {
  it("shows everything until the adapter has answered", () => {
    expect(itemsFor("markdown", []).map((i) => i.id)).toEqual([
      "bold",
      "italic",
      "code",
      "strike",
      "heading",
      "bullets",
      "numbers",
      "quote",
      "link",
      "image",
      "math",
      "rule",
    ]);
  });

  it("offers only what the adapter lists, and Typst inserts a figure", () => {
    const typst = ALL.filter(
      (s) => !["strike", "quote", "rule", "image"].includes(s.command),
    );
    const items = itemsFor("typst", typst);
    expect(items.map((i) => i.id)).toEqual([
      "bold",
      "italic",
      "code",
      "heading",
      "bullets",
      "numbers",
      "link",
      "image",
      "math",
    ]);
    expect(items.find((i) => i.id === "image")?.command).toBe("figure");
    expect(itemsFor("markdown", ALL).find((i) => i.id === "image")?.command).toBe(
      "image",
    );
  });

  it("keeps a group divider only between groups that exist", () => {
    const onlyInline = ALL.filter((s) => ["bold", "italic"].includes(s.command));
    expect(itemsFor("markdown", onlyInline).map((i) => i.groupStart)).toEqual([
      false,
      false,
    ]);
    const mixed = ALL.filter((s) => ["bold", "link"].includes(s.command));
    expect(itemsFor("markdown", mixed).map((i) => i.groupStart)).toEqual([false, true]);
  });

  it("reads pressed and disabled from the command state", () => {
    const states: CommandState[] = [
      { command: "bold", enabled: true, active: true },
      { command: "quote", enabled: false, active: false },
    ];
    const items = itemsFor("markdown", []);
    const bold = items.find((i) => i.id === "bold");
    const heading = items.find((i) => i.id === "heading");
    expect(bold && isPressed(states, bold, 0)).toBe(true);
    expect(heading && isPressed(states, heading, 2)).toBe(true);
    expect(heading && isPressed(states, heading, 0)).toBe(false);
    expect(isDisabled(states, "quote")).toBe(true);
    expect(isDisabled(states, "bold")).toBe(false);
    expect(isDisabled(states, "unknown")).toBe(false);
  });

  it("labels tooltips with their shortcut", () => {
    expect(tooltip("Bold", "Ctrl+B")).toBe("Bold (Ctrl+B)");
    expect(tooltip("Quote", undefined)).toBe("Quote");
  });

  it("clears a heading by choosing its level again", () => {
    expect(headingArg(2, 2)).toBe("0");
    expect(headingArg(2, 3)).toBe("3");
    expect(headingArg(0, 1)).toBe("1");
  });

  it("writes image sources relative to the document", () => {
    expect(relativeTo("notes.md", "figures/a.svg")).toBe("figures/a.svg");
    expect(relativeTo("sections/results.typ", "figures/a.svg")).toBe(
      "../figures/a.svg",
    );
    expect(relativeTo("sections/results.typ", "sections/img/a.png")).toBe("img/a.png");
    expect(relativeTo("a/b/c.md", "a/x.png")).toBe("../x.png");
  });
});

describe("a plan is one transaction", () => {
  const apply = (
    text: string,
    anchor: number,
    head: number,
    command: string,
    arg?: string,
  ) => {
    let state = EditorState.create({
      doc: text,
      selection: { anchor, head },
      extensions: [history()],
    });
    const edit = plan(
      { format: "markdown", text },
      command,
      Math.min(anchor, head),
      Math.max(anchor, head),
      arg,
    );
    expect(planFits(edit, state.doc.length)).toBe(true);
    const tr = state.update(planToSpec(edit));
    state = tr.state;
    return { state, tr, edit };
  };

  it("applies changes and selection together, in the new document", () => {
    const { state, tr } = apply("one two three", 4, 7, "bold");
    expect(state.doc.toString()).toBe("one **two** three");
    expect(state.selection.main).toMatchObject({ from: 6, to: 9 });
    expect(tr.isUserEvent("input.format")).toBe(true);
  });

  it("undoes in a single step, even after typing before it", () => {
    let { state } = apply("one two three", 4, 7, "bold");
    // Typing right before the action stays a separate undo step.
    state = state.update({
      changes: { from: state.doc.length, insert: "!" },
      userEvent: "input.type",
    }).state;
    let undone = state;
    undo({
      state: undone,
      dispatch: (tr) => (undone = tr.state),
    });
    expect(undone.doc.toString()).toBe("one **two** three");
    undo({
      state: undone,
      dispatch: (tr) => (undone = tr.state),
    });
    expect(undone.doc.toString()).toBe("one two three");
    let redone = undone;
    redo({ state: undone, dispatch: (tr) => (redone = tr.state) });
    expect(redone.doc.toString()).toBe("one **two** three");
  });

  it("refuses a plan that does not fit the document", () => {
    const bad = {
      changes: [{ from: 50, to: 50, insert: "x" }],
      selection: null,
    };
    expect(planFits(bad, 10)).toBe(false);
  });
});

describe("visual mode rules", () => {
  it("knows which files have a Visual mode", () => {
    expect(visualFormat("notes.md")).toBe("markdown");
    expect(visualFormat("a/b.markdown")).toBe("markdown");
    expect(visualFormat("main.typ")).toBe("typst");
    expect(visualFormat("paper.tex")).toBeNull();
    expect(visualFormat("README")).toBeNull();
  });

  it("reads and writes the per-type default setting", () => {
    expect(opensVisual([], "markdown")).toBe(false);
    expect(opensVisual(["typst"], "markdown")).toBe(false);
    expect(opensVisual(["markdown", "typst"], "typst")).toBe(true);
    expect(opensVisual("markdown", "markdown")).toBe(false);
    expect(withVisualDefault([], "markdown", true)).toEqual(["markdown"]);
    expect(withVisualDefault(["markdown"], "typst", true)).toEqual([
      "markdown",
      "typst",
    ]);
    expect(withVisualDefault(["markdown", "typst"], "markdown", false)).toEqual([
      "typst",
    ]);
    expect(withVisualDefault(["markdown", "markdown", "x"], "markdown", true)).toEqual([
      "markdown",
    ]);
    expect(withVisualDefault(undefined, "typst", false)).toEqual([]);
  });

  it("resolves image sources against the document and the project", () => {
    expect(resolveAsset("notes.md", "figures/a.svg")).toBe("figures/a.svg");
    expect(resolveAsset("sections/r.typ", "../figures/a.svg")).toBe("figures/a.svg");
    expect(resolveAsset("sections/r.typ", "/figures/a.svg")).toBe("figures/a.svg");
    expect(resolveAsset("a.md", "./x.png?raw")).toBe("x.png");
    expect(resolveAsset("a.md", "../../x.png")).toBeNull();
    expect(resolveAsset("a.md", "https://example.org/x.png")).toBeNull();
    expect(resolveAsset("a.md", "data:image/png;base64,AAAA")).toBeNull();
    expect(resolveAsset("a.md", "")).toBeNull();
  });

  it("finds the scene behind an exported SVG", () => {
    const scenes = new Set(["figures/a.figure", "figures/b.figure.json"]);
    expect(figureSceneFor("figures/a.svg", scenes)).toBe("figures/a.figure");
    expect(figureSceneFor("figures/b.svg", scenes)).toBe("figures/b.figure.json");
    expect(figureSceneFor("figures/c.svg", scenes)).toBeNull();
    expect(figureSceneFor("figures/a.png", scenes)).toBeNull();
  });
});
