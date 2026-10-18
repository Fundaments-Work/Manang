import { describe, expect, it } from "vitest";

import { feedbackText } from "./feedback";

describe("feedback text", () => {
  it("is the message under a heading", () => {
    expect(feedbackText("feedback", "  Nice  ", "")).toBe("Manang feedback\n\nNice\n");
  });

  it("says when there is no message", () => {
    expect(feedbackText("problem", " ", "")).toBe(
      "Manang problem report\n\n(no message)\n",
    );
  });

  it("adds the diagnostics after the message when they are included", () => {
    const text = feedbackText("problem", "It crashed", "Manang 0.1.0\n");
    expect(text).toBe(
      "Manang problem report\n\nIt crashed\n\n--- diagnostics ---\n\nManang 0.1.0\n",
    );
  });
});
