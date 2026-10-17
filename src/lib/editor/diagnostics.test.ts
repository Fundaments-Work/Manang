import { describe, expect, it } from "vitest";

import {
  analyse,
  applyFix,
  definedLabels,
  lineOf,
  type Diagnostic,
} from "./diagnostics";

function only(diagnostics: Diagnostic[], fragment: string): Diagnostic {
  const found = diagnostics.find((entry) => entry.message.includes(fragment));
  if (found === undefined) {
    throw new Error(
      `no diagnostic containing "${fragment}" in: ${diagnostics
        .map((entry) => entry.message)
        .join(", ")}`,
    );
  }
  return found;
}

describe("definedLabels", () => {
  it("collects every label definition", () => {
    const labels = definedLabels('#label("a")\n#let x = 1\n#label("b")');
    expect([...labels]).toEqual(["a", "b"]);
  });

  it("tolerates whitespace inside the call", () => {
    expect(definedLabels('#label( "spaced" )').has("spaced")).toBe(true);
  });
});

describe("analyse: math", () => {
  it("stays silent on balanced delimiters", () => {
    const found = analyse("Energy $E = mc^2$ holds.");
    expect(found).toHaveLength(0);
  });

  it("flags an odd delimiter count", () => {
    const found = analyse("Broken $E = mc^2");
    expect(only(found, "math").severity).toBe("error");
  });

  it("offers a fix that closes the delimiter", () => {
    const text = "Broken $E = mc^2";
    const diagnostic = only(analyse(text), "math");
    expect(applyFix(text, diagnostic)).toBe(`${text}$`);
  });

  it("ignores math inside fenced code", () => {
    const found = analyse("```typst\n$x = 1\n```\n");
    expect(found).toHaveLength(0);
  });
});

describe("analyse: references", () => {
  it("flags a reference with no label", () => {
    const found = analyse("As shown in @landauer1961.");
    expect(only(found, "Missing label").message).toBe(
      "Missing label for @landauer1961",
    );
  });

  it("stays silent when the label is defined", () => {
    const found = analyse('#label("landauer1961")\n\nSee @landauer1961.');
    expect(found).toHaveLength(0);
  });

  it("adds a label definition on fix", () => {
    const text = "As shown in @penrose.";
    const fixed = applyFix(text, only(analyse(text), "Missing label"));
    expect(fixed).toContain('#label("penrose")');
    // The fix must make the diagnostic disappear.
    expect(analyse(fixed ?? "")).toHaveLength(0);
  });
});

describe("analyse: brackets", () => {
  it("flags an unclosed code block", () => {
    const found = analyse("Intro\n\n#{ let x = 1");
    expect(only(found, "Unclosed").message).toContain("}");
  });

  it("flags an unclosed content block", () => {
    const found = analyse("Intro\n\n#( let x = 1");
    expect(only(found, "Unclosed").message).toContain("#(");
  });

  it("flags an unclosed bracket", () => {
    expect(only(analyse("See [1"), "Unclosed").message).toContain("]");
  });

  it("stays silent on balanced brackets", () => {
    expect(analyse("#{ let x = (1 + 2) }")).toHaveLength(0);
  });
});

describe("analyse: figures", () => {
  it("flags an image with no matching file", () => {
    const found = analyse('#image("figures/missing.svg")', {
      knownFigures: ["figures/present.svg"],
    });
    expect(only(found, "Figure not found").message).toContain("figures/missing.svg");
  });

  it("reads the path when further arguments follow", () => {
    // The embed snippet DESIGN.md §4.1 generates always carries `width`.
    const text = '#figure(image("figures/missing.svg", width: 80%))';
    const found = analyse(text, { knownFigures: ["figures/present.svg"] });
    expect(only(found, "Figure not found").message).toContain("figures/missing.svg");
  });

  it("stays silent when the figure exists", () => {
    const text = '#image("figures/present.svg")';
    expect(analyse(text, { knownFigures: ["figures/present.svg"] })).toHaveLength(0);
  });

  it("does not check figures when the library is empty", () => {
    expect(analyse('#image("figures/any.svg")')).toHaveLength(0);
  });
});

describe("analyse: ordering and offsets", () => {
  it("returns diagnostics sorted by position", () => {
    const found = analyse("Intro\n@missing\n$x\n#(");
    const offsets = found.map((entry) => entry.from);
    expect([...offsets].sort((a, b) => a - b)).toEqual(offsets);
  });

  it("reports offsets inside the original text", () => {
    const text = "Line one\n@ghost";
    const diagnostic = only(analyse(text), "Missing label");
    expect(text.slice(diagnostic.from, diagnostic.to)).toBe("@ghost");
  });
});

describe("applyFix", () => {
  it("returns null when no fix is offered", () => {
    const text = '#image("figures/missing.svg")';
    const diagnostic = only(
      analyse(text, { knownFigures: ["figures/present.svg"] }),
      "Figure not found",
    );
    expect(applyFix(text, diagnostic)).toBeNull();
  });

  it("returns null when the document has shrunk past the diagnostic", () => {
    const text = "@ghost";
    const diagnostic = only(analyse(text), "Missing label");
    expect(applyFix("", diagnostic)).toBeNull();
  });
});

describe("lineOf", () => {
  it("counts from one", () => {
    expect(lineOf("a\nb\nc", 0)).toBe(1);
  });

  it("points at the line containing the offset", () => {
    expect(lineOf("a\nb\nc", 2)).toBe(2);
    expect(lineOf("a\nb\nc", 4)).toBe(3);
  });

  it("clamps to the last line past the end of the document", () => {
    expect(lineOf("a\nb", 999)).toBe(2);
  });
});
