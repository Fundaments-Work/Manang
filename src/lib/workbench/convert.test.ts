import { describe, expect, it } from "vitest";

import { convertedMessage, typstPathFor } from "./convert";

describe("Markdown to Typst conversion", () => {
  it("puts the Typst file next to the Markdown one", () => {
    expect(typstPathFor("notes.md")).toBe("notes.typ");
    expect(typstPathFor("docs/ch.1.markdown")).toBe("docs/ch.1.typ");
    expect(typstPathFor("main.typ")).toBeNull();
    expect(typstPathFor("references.bib")).toBeNull();
  });

  it("counts only what did not convert exactly", () => {
    expect(convertedMessage("notes.typ", [])).toBe("Converted to notes.typ.");
    expect(convertedMessage("a/notes.typ", [{ severity: "info" }])).toBe(
      "Converted to notes.typ.",
    );
    expect(
      convertedMessage("notes.typ", [{ severity: "warning" }, { severity: "info" }]),
    ).toBe("Converted to notes.typ; 1 part could not be converted exactly.");
    expect(
      convertedMessage("notes.typ", [{ severity: "warning" }, { severity: "error" }]),
    ).toBe("Converted to notes.typ; 2 parts could not be converted exactly.");
  });
});
