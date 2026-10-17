import { describe, expect, it } from "vitest";

import { writingSyntaxFor } from "./languageMap";

describe("writingSyntaxFor", () => {
  it("passes through the markup the writing plugin understands", () => {
    expect(writingSyntaxFor("typst")).toBe("typst");
    expect(writingSyntaxFor("latex")).toBe("latex");
    expect(writingSyntaxFor("markdown")).toBe("markdown");
  });

  it("treats everything else, including BibTeX, as plain text", () => {
    expect(writingSyntaxFor("bibtex")).toBe("plain");
    expect(writingSyntaxFor("plain")).toBe("plain");
    expect(writingSyntaxFor(null)).toBe("plain");
    expect(writingSyntaxFor(undefined)).toBe("plain");
  });
});
