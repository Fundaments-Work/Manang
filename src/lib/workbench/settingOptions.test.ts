import { describe, expect, it } from "vitest";

import {
  citationStyleOf,
  declaredOptions,
  languageName,
  optionLabel,
  spellLanguageLabel,
  spellLanguageOptions,
} from "./settingOptions";

describe("setting option labels", () => {
  it("names spelling languages, falling back to the code", () => {
    expect(languageName("en_US")).toBe("American English");
    expect(languageName("en_GB")).toBe("British English");
    expect(languageName("not a tag")).toBe("not a tag");
  });

  it("marks dictionaries from the config folder", () => {
    expect(spellLanguageLabel({ code: "en_GB", source: "bundled" })).toBe(
      "British English",
    );
    expect(spellLanguageLabel({ code: "de_DE", source: "user" })).toBe(
      "German (Germany) (your dictionary)",
    );
  });

  it("labels citation styles and other values readably", () => {
    expect(optionLabel("citations.style", "apa")).toBe("APA");
    expect(optionLabel("citations.style", "numeric")).toBe("Numeric (IEEE)");
    expect(optionLabel("writing.spell-language", "en_US")).toBe("American English");
    expect(optionLabel("latex.engine", "xelatex")).toBe("XeLaTeX");
    expect(declaredOptions("appearance.theme", ["dark", "light"])).toEqual([
      { value: "dark", label: "Dark" },
      { value: "light", label: "Light" },
    ]);
    expect(declaredOptions("x.y", undefined)).toEqual([]);
  });

  it("offers the reported dictionaries the schema accepts", () => {
    const reported = [
      { code: "de_DE", source: "user" },
      { code: "en_GB", source: "bundled" },
      { code: "en_US", source: "bundled" },
    ] as const;
    expect(spellLanguageOptions(reported, ["en_US", "en_GB"])).toEqual([
      { value: "en_GB", label: "British English" },
      { value: "en_US", label: "American English" },
    ]);
    expect(spellLanguageOptions(reported, undefined)?.map((o) => o.value)).toEqual([
      "de_DE",
      "en_GB",
      "en_US",
    ]);
    expect(spellLanguageOptions([], ["en_US"])).toBeUndefined();
    expect(spellLanguageOptions(reported, ["fr_FR"])).toBeUndefined();
  });

  it("reads citation styles from setting values", () => {
    expect(citationStyleOf("apa")).toBe("apa");
    expect(citationStyleOf("numeric")).toBe("numeric");
    expect(citationStyleOf("ieee")).toBe("ieee");
    expect(citationStyleOf("chicago")).toBeUndefined();
    expect(citationStyleOf(undefined)).toBeUndefined();
  });
});
