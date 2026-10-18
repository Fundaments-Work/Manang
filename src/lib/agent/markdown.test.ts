import { describe, expect, it } from "vitest";

import { readiness } from "./readiness";
import { parseInline, parseMarkdown } from "./markdown";
import type { AgentStatus } from "./types";

describe("parseMarkdown", () => {
  it("reads paragraphs, lists and headings", () => {
    const blocks = parseMarkdown(
      "# Title\n\nOne two\nthree.\n\n- a\n- b\n\n1. x\n2. y",
    );
    expect(blocks.map((b) => b.kind)).toEqual(["heading", "paragraph", "list", "list"]);
    const first = blocks[2];
    expect(first?.kind === "list" && first.items.length).toBe(2);
    const second = blocks[3];
    expect(second?.kind === "list" && second.ordered).toBe(true);
  });

  it("keeps fenced code verbatim and flags an unfinished fence", () => {
    const done = parseMarkdown("```typst\n= Hi\n*a*\n```");
    expect(done).toEqual([
      { kind: "code", lang: "typst", text: "= Hi\n*a*", open: false },
    ]);
    const streaming = parseMarkdown("text\n\n```\nlet x");
    expect(streaming[1]).toMatchObject({ kind: "code", open: true, text: "let x" });
  });

  it("never produces markup: html stays text", () => {
    const [block] = parseMarkdown("<img src=x onerror=alert(1)> **b**");
    expect(block?.kind).toBe("paragraph");
    const inline = block?.kind === "paragraph" ? block.inline : [];
    expect(inline[0]).toEqual({ kind: "text", text: "<img src=x onerror=alert(1)> " });
    expect(inline[1]).toEqual({ kind: "strong", text: "b" });
  });

  it("handles inline code, emphasis and plain text", () => {
    expect(parseInline("use `#set` and *this*")).toEqual([
      { kind: "text", text: "use " },
      { kind: "code", text: "#set" },
      { kind: "text", text: " and " },
      { kind: "em", text: "this" },
    ]);
  });

  it("does not throw on empty or odd input", () => {
    expect(parseMarkdown("")).toEqual([]);
    expect(() => parseMarkdown("```\n\n***\n- \n>")).not.toThrow();
  });
});

const status = (over: Partial<AgentStatus>): AgentStatus =>
  ({
    enabled: true,
    ready: false,
    problems: [],
    presets: [],
    ...over,
  }) as unknown as AgentStatus;

describe("readiness", () => {
  it("names the four states", () => {
    expect(readiness(null)).toBe("checking");
    expect(readiness(status({ ready: true }))).toBe("ready");
    expect(readiness(status({ enabled: false, problems: ["disabled"] }))).toBe("off");
    expect(readiness(status({ problems: ["key_missing"] }))).toBe("not-set-up");
    expect(readiness(status({ problems: ["consent_required"] }))).toBe("not-set-up");
  });
});
