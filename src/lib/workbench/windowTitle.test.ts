import { describe, expect, it } from "vitest";

import { windowTitle } from "./windowTitle";

describe("windowTitle", () => {
  it("names the file and the project in the workbench", () => {
    expect(
      windowTitle({
        surface: "workbench",
        project: "Entropy paper",
        file: "sections/main.typ",
      }),
    ).toBe("main.typ — Entropy paper");
  });

  it("names just the project when no file is open", () => {
    expect(
      windowTitle({ surface: "workbench", project: "Entropy paper", file: null }),
    ).toBe("Entropy paper");
  });

  it("names the page on Home and in Settings", () => {
    expect(
      windowTitle({ surface: "home", project: "Entropy paper", file: "a.typ" }),
    ).toBe("Home");
    expect(
      windowTitle({ surface: "settings", project: "Entropy paper", file: "a.typ" }),
    ).toBe("Settings — Entropy paper");
    expect(windowTitle({ surface: "settings", project: null, file: null })).toBe(
      "Settings",
    );
  });

  it("is the app name when nothing is open", () => {
    expect(windowTitle({ surface: "workbench", project: null, file: null })).toBe(
      "Manang",
    );
  });
});
