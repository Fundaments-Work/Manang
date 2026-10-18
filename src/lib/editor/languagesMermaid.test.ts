import { StringStream } from "@codemirror/language";
import { describe, expect, it } from "vitest";

import { languageFor, loadLanguageExtension } from "./languages";
import { mermaidParser } from "./languagesMermaid";

/** `[text, style]` pairs of one line, skipping whitespace. */
function tokens(line: string): [string, string | null][] {
  const state = mermaidParser.startState?.(2) ?? { label: null };
  const stream = new StringStream(line, 2, 2);
  const out: [string, string | null][] = [];
  while (!stream.eol()) {
    stream.start = stream.pos;
    const style = mermaidParser.token(stream, state);
    const text = stream.current();
    if (text.trim() !== "") out.push([text, style]);
  }
  return out;
}

describe("mermaid mode", () => {
  it("is chosen for .mmd and .mermaid", () => {
    expect(languageFor("figures/flow.mmd")).toBe("mermaid");
    expect(languageFor("a.mermaid")).toBe("mermaid");
    expect(languageFor("a.pikchr")).toBe("plain");
  });

  it("loads an extension", async () => {
    expect(await loadLanguageExtension("mermaid")).toBeTruthy();
  });

  it("marks keywords, ids, arrows and labels", () => {
    expect(tokens("flowchart TD")).toEqual([
      ["flowchart", "keyword"],
      ["TD", "keyword"],
    ]);
    expect(tokens("A[Start here] --> B")).toEqual([
      ["A", "variable"],
      ["[", "bracket"],
      ["Start here", "string"],
      ["]", "bracket"],
      ["-->", "operator"],
      ["B", "variable"],
    ]);
  });

  it("marks comments, strings and edge labels", () => {
    expect(tokens("%% a note")).toEqual([["%% a note", "comment"]]);
    expect(tokens('A -->|yes| B["q"]')[2]).toEqual(["|yes|", "string"]);
    expect(tokens("Alice->>Bob: hi")[1]).toEqual(["->>", "operator"]);
  });
});
