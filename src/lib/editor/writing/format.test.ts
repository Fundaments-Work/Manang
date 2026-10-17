import { EditorSelection, EditorState, type TransactionSpec } from "@codemirror/state";
import { describe, expect, it } from "vitest";

import { writingApi } from "$lib/kernel/api";
import { createMockKernel } from "$lib/kernel/mock";
import { writingPlugin } from "$lib/kernel/mockWriting";

import { formatView, LATEX_UNSUPPORTED, planFormat } from "./format";

function apply(text: string, plan: ReturnType<typeof planFormat>): string {
  const state = EditorState.create({ doc: text });
  return state
    .update({ changes: plan.changes.map((c) => ({ ...c })) })
    .state.doc.toString();
}

describe("planFormat", () => {
  const cases: [string, string][] = [
    ["a  \nb \nc", "a\nb\nc"],
    ["a\n\n\n\nb", "a\n\nb"],
    ["a", "a\n"],
    ["x\ny\nz\n", "x\nz\n"],
    ["😀 a  \n😀 b", "😀 a\n😀 b"],
    ["", "hello\n"],
    ["same", "same"],
    ["abc\ndef", "abXc\nde"],
  ];

  it.each(cases)("turns %j into %j", (before, after) => {
    expect(apply(before, planFormat(before, after, 0))).toBe(after);
  });

  it("applies exactly, for random line edits including inserts and deletes", () => {
    let seed = 12345;
    const rand = (n: number): number => {
      seed = (Math.imul(seed, 1103515245) + 12345) & 0x7fffffff;
      return seed % n;
    };
    const pool = ["", "alpha", "beta  ", "γ δ", "😀 x", "  indented", "a b c", "z"];
    const lines = (): string[] =>
      Array.from({ length: rand(9) }, () => pool[rand(pool.length)] ?? "");
    for (let round = 0; round < 400; round += 1) {
      const before = lines().join("\n");
      const after = lines().join("\n");
      const head = rand(before.length + 1);
      const plan = planFormat(before, after, head);
      expect(apply(before, plan)).toBe(after);
      expect(plan.cursor).toBeGreaterThanOrEqual(0);
      expect(plan.cursor).toBeLessThanOrEqual(after.length);
    }
  });

  it("emits per-line edits when the line count is unchanged", () => {
    const plan = planFormat("a  \nkeep\nb  ", "a\nkeep\nb", 0);
    expect(plan.changes).toHaveLength(2);
  });

  it("keeps a caret in an untouched line where it was, shifted by earlier edits", () => {
    const before = "a  \n\n\n\nkeep this";
    const after = "a\n\nkeep this";
    const head = before.indexOf("this");
    const plan = planFormat(before, after, head);
    expect(after.slice(plan.cursor, plan.cursor + 4)).toBe("this");
  });

  it("keeps the column on a reformatted line", () => {
    const before = "hello   world";
    const after = "hello world";
    const plan = planFormat(before, after, 3);
    expect(plan.cursor).toBe(3);
    const end = planFormat(before, after, before.length);
    expect(end.cursor).toBe(after.length);
  });

  it("never splits a surrogate pair", () => {
    const before = "x😀y";
    const after = "x😁y";
    const plan = planFormat(before, after, 0);
    expect(apply(before, plan)).toBe(after);
    for (const c of plan.changes) {
      expect(c.insert).not.toMatch(/^[\udc00-\udfff]|[\ud800-\udbff]$/);
    }
  });
});

/** A view stand-in that counts dispatches. */
function fakeView(text: string, selection?: EditorSelection) {
  let state = EditorState.create(
    selection === undefined ? { doc: text } : { doc: text, selection },
  );
  const dispatched: TransactionSpec[] = [];
  return {
    get state() {
      return state;
    },
    dispatch(spec: TransactionSpec) {
      dispatched.push(spec);
      state = state.update(spec).state;
    },
    dispatched,
  };
}

const api = () => writingApi(createMockKernel({ plugins: [writingPlugin()] }));

describe("formatView", () => {
  it("applies the whole result as ONE transaction", async () => {
    const view = fakeView("a  \nb  \n\n\n\nc  ");
    const outcome = await formatView(view as never, api(), "typst");
    expect(outcome).toEqual({ kind: "formatted" });
    expect(view.dispatched).toHaveLength(1);
    expect(view.state.doc.toString()).toBe("a\nb\n\nc\n");
  });

  it("keeps the caret in place", async () => {
    const text = "first line  \nsecond line  \nthird";
    const head = text.indexOf("cond");
    const view = fakeView(text, EditorSelection.single(head));
    await formatView(view as never, api(), "markdown");
    expect(
      view.state.sliceDoc(
        view.state.selection.main.head,
        view.state.selection.main.head + 4,
      ),
    ).toBe("cond");
  });

  it("reports unchanged text without dispatching", async () => {
    const view = fakeView("clean\n");
    expect(await formatView(view as never, api(), "typst")).toEqual({
      kind: "unchanged",
    });
    expect(view.dispatched).toHaveLength(0);
  });

  it("says LaTeX is not supported yet without calling the kernel", async () => {
    const view = fakeView("\\section{A}  ");
    const outcome = await formatView(view as never, api(), "latex");
    expect(outcome).toEqual({ kind: "unsupported", message: LATEX_UNSUPPORTED });
    expect(view.dispatched).toHaveLength(0);
  });

  it("does not clobber text typed while the formatter ran", async () => {
    const view = fakeView("a  ");
    const slow = {
      ...api(),
      format: async () => {
        view.dispatch({ changes: { from: 3, insert: "!" } });
        return { formatted: "a", changed: true };
      },
    };
    const outcome = await formatView(view as never, slow as never, "typst");
    expect(outcome).toEqual({ kind: "stale" });
    expect(view.state.doc.toString()).toBe("a  !");
  });

  it("passes a declined format through as skipped", async () => {
    const view = fakeView("#let x = ");
    const declining = {
      ...api(),
      format: () =>
        Promise.resolve({
          formatted: "#let x = ",
          changed: false,
          skipped: "syntax errors",
        }),
    };
    expect(await formatView(view as never, declining as never, "typst")).toEqual({
      kind: "skipped",
      message: "syntax errors",
    });
  });
});
