import { CompletionContext } from "@codemirror/autocomplete";
import { EditorState } from "@codemirror/state";
import { describe, expect, it } from "vitest";

import { writingApi } from "$lib/kernel/api";
import { createMockKernel } from "$lib/kernel/mock";
import { writingPlugin } from "$lib/kernel/mockWriting";

import {
  inMath,
  snippetSource,
  snippetTemplate,
  symbolQuery,
  symbolSource,
} from "./complete";

const api = () => writingApi(createMockKernel({ plugins: [writingPlugin()] }));

const stateAt = (text: string) => {
  const pos = text.indexOf("|");
  const doc = text.replace("|", "");
  return { state: EditorState.create({ doc }), pos };
};

describe("inMath", () => {
  it("is true between an odd number of dollars in the paragraph", () => {
    const { state, pos } = stateAt("text $a + al|");
    expect(inMath(state, pos)).toBe(true);
  });

  it("is false after the formula closed or in a later paragraph", () => {
    expect(
      inMath(...(Object.values(stateAt("$a$ and al|")) as [EditorState, number])),
    ).toBe(false);
    const { state, pos } = stateAt("$unclosed\n\nnew paragraph al|");
    expect(inMath(state, pos)).toBe(false);
  });

  it("ignores escaped dollars", () => {
    const { state, pos } = stateAt("costs \\$5 and al|");
    expect(inMath(state, pos)).toBe(false);
  });
});

describe("symbolQuery", () => {
  it("LaTeX needs a backslash and returns the name after it", () => {
    const { state, pos } = stateAt("so \\alp|");
    expect(symbolQuery(state, pos, "latex", false)).toEqual({ from: 3, query: "alp" });
    const none = stateAt("so alp|");
    expect(symbolQuery(none.state, none.pos, "latex", false)).toBeNull();
  });

  it("Typst completes dotted names only inside math, from two letters", () => {
    const inside = stateAt("$ x -> arrow.r|");
    expect(symbolQuery(inside.state, inside.pos, "typst", false)).toEqual({
      from: 7,
      query: "arrow.r",
    });
    const prose = stateAt("plain arrow|");
    expect(symbolQuery(prose.state, prose.pos, "typst", false)).toBeNull();
    const short = stateAt("$ a|");
    expect(symbolQuery(short.state, short.pos, "typst", false)).toBeNull();
    expect(symbolQuery(short.state, short.pos, "typst", true)).not.toBeNull();
  });
});

describe("snippetTemplate", () => {
  it("builds CodeMirror fields, mirrors and the final cursor", () => {
    const template = snippetTemplate({
      text: "x = y; x\n",
      tab_stops: [
        {
          index: 1,
          placeholder: "x",
          ranges: [
            { start: 0, end: 1 },
            { start: 7, end: 8 },
          ],
        },
        { index: 2, placeholder: "y", ranges: [{ start: 4, end: 5 }] },
      ],
      final_cursor: 9,
      trigger: "t",
      name: "T",
      requires: null,
    });
    expect(template).toBe("${1:x} = ${2:y}; ${1:x}\n${0}");
  });

  it("escapes literal braces so Typst blocks are not read as fields", () => {
    const template = snippetTemplate({
      text: "#{ a } ${b}",
      tab_stops: [{ index: 1, placeholder: "q", ranges: [{ start: 9, end: 10 }] }],
      final_cursor: 11,
      trigger: "t",
      name: "T",
      requires: null,
    });
    expect(template).toContain("#\\{ a \\} $\\{${1:b}\\}");
  });

  it("counts positions in scalars, not UTF-16 units", () => {
    const template = snippetTemplate({
      text: "😀 name",
      tab_stops: [{ index: 1, placeholder: "name", ranges: [{ start: 2, end: 6 }] }],
      final_cursor: 6,
      trigger: "t",
      name: "T",
      requires: null,
    });
    expect(template).toBe("😀 ${1:name}${0}");
  });

  it("drops a placeholder CodeMirror cannot carry but keeps its text", () => {
    const template = snippetTemplate({
      text: "a{b}c",
      tab_stops: [{ index: 1, placeholder: "{b}", ranges: [{ start: 1, end: 4 }] }],
      final_cursor: 5,
      trigger: "t",
      name: "T",
      requires: null,
    });
    expect(template).toBe("a\\{b\\}c${0}");
  });
});

const asTypst = () => "typst" as const;
const asLatex = () => "latex" as const;

describe("symbol source", () => {
  it("offers the symbol's own syntax with the glyph as preview", async () => {
    const { state, pos } = stateAt("$ x = alph|");
    const result = await symbolSource(
      api(),
      asTypst,
    )(new CompletionContext(state, pos, false));
    const first = result?.options[0];
    expect(first?.label).toBe("alpha");
    expect(first?.detail).toBe("α");
    expect(first?.apply).toBe("alpha");
    expect(result?.from).toBe(6);
    expect(result?.filter).toBe(false);
  });

  it("inserts the LaTeX command, replacing the backslash too", async () => {
    const { state, pos } = stateAt("so \\int|");
    const result = await symbolSource(
      api(),
      asLatex,
    )(new CompletionContext(state, pos, false));
    expect(result?.from).toBe(3);
    expect(result?.options[0]?.apply).toBe("\\int");
    expect(result?.options[0]?.detail).toBe("∫");
  });

  it("stays quiet in prose and when the syntax is unknown", async () => {
    const prose = stateAt("just words alph|");
    expect(
      await symbolSource(
        api(),
        asTypst,
      )(new CompletionContext(prose.state, prose.pos, false)),
    ).toBeNull();
    const math = stateAt("$ alph|");
    expect(
      await symbolSource(api())(new CompletionContext(math.state, math.pos, false)),
    ).toBeNull();
  });

  it("does not repeat a request for a query it has answered", async () => {
    const kernel = createMockKernel({ plugins: [writingPlugin()] });
    let calls = 0;
    const counting = {
      ...kernel,
      run: <T>(c: string, a?: Record<string, unknown>) => {
        calls += 1;
        return kernel.run<T>(c, a);
      },
    };
    const source = symbolSource(writingApi(counting), asTypst);
    const { state, pos } = stateAt("$ alph|");
    await source(new CompletionContext(state, pos, false));
    await source(new CompletionContext(state, pos, false));
    expect(calls).toBe(1);
  });
});

describe("snippet source", () => {
  it("lists the snippets of the document's syntax, aliases included", async () => {
    const { state, pos } = stateAt("the|");
    const result = await snippetSource(
      api(),
      asTypst,
    )(new CompletionContext(state, pos, false));
    const labels = result?.options.map((o) => o.label);
    expect(labels).toContain("theorem");
    expect(labels).toContain("thm");
  });

  it("is quiet when the syntax is unknown or the word is one letter", async () => {
    const { state, pos } = stateAt("theo|");
    expect(
      await snippetSource(api())(new CompletionContext(state, pos, false)),
    ).toBeNull();
    const one = stateAt("t|");
    expect(
      await snippetSource(
        api(),
        asTypst,
      )(new CompletionContext(one.state, one.pos, false)),
    ).toBeNull();
  });
});
