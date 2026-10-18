import { describe, expect, it } from "vitest";

import {
  LARGE_TEXT_BYTES,
  describeFile,
  formatModified,
  formatSize,
  looksLikeText,
  needsProbe,
  openAsByContent,
  openAsByName,
  tabKindOf,
} from "./opener";
import { kindForPath, openTab, renamePath, EMPTY_TABS } from "./tabs";

const utf8 = (text: string): Uint8Array => new TextEncoder().encode(text);

describe("what opens a file", () => {
  it("decides by name when the name is enough", () => {
    expect(openAsByName("figures/a.figure.json")).toBe("figure");
    expect(openAsByName("main.typ")).toBe("text");
    expect(openAsByName("refs.BIB")).toBe("text");
    expect(openAsByName("thesis.cls")).toBe("text");
    expect(openAsByName("manang.toml")).toBe("text");
    expect(openAsByName("plot.py")).toBe("text");
    expect(openAsByName("graph.dot")).toBe("text");
    expect(openAsByName("paper.pdf")).toBe("pdf");
    for (const image of [
      "a.png",
      "a.JPG",
      "a.jpeg",
      "a.gif",
      "a.webp",
      "a.svg",
      "a.bmp",
      "a.avif",
      "a.ico",
    ]) {
      expect(openAsByName(image)).toBe("image");
    }
    for (const binary of ["a.zip", "a.docx", "a.xlsx", "a.woff2", "a.mp3", "a.mp4"]) {
      expect(openAsByName(binary)).toBe("binary");
    }
  });

  it("leaves unknown names to the bytes, and probes data-like text for size", () => {
    expect(openAsByName("README")).toBeNull();
    expect(openAsByName("notes.weird")).toBeNull();
    expect(openAsByName(".gitignore")).toBeNull();
    expect(needsProbe("README")).toBe(true);
    expect(needsProbe("data/run.csv")).toBe(true);
    expect(needsProbe("server.log")).toBe(true);
    expect(needsProbe("main.typ")).toBe(false);
    expect(needsProbe("a.png")).toBe(false);
  });

  it("sniffs text: valid UTF-8 without NUL", () => {
    expect(looksLikeText(utf8("plain words, ünïcödé, ∑"))).toBe(true);
    expect(looksLikeText(new Uint8Array([]))).toBe(true);
    expect(looksLikeText(new Uint8Array([0x41, 0x00, 0x42]))).toBe(false);
    expect(looksLikeText(new Uint8Array([0xff, 0xfe, 0x41]))).toBe(false);
    // A multi-byte character cut by the sniff window is still text.
    const cut = utf8("ab∑").subarray(0, 4);
    expect(looksLikeText(cut)).toBe(true);
  });

  it("decides probed files by content and size", () => {
    expect(openAsByContent("README", utf8("# Read me\n"))).toBe("text");
    expect(
      openAsByContent("blob", new Uint8Array([0x50, 0x4b, 0x03, 0x04, 0x00])),
    ).toBe("binary");
    expect(openAsByContent("run.csv", utf8("a,b\n1,2\n"))).toBe("text");
    const big = new Uint8Array(LARGE_TEXT_BYTES + 1).fill(0x61);
    expect(openAsByContent("run.csv", big)).toBe("large-text");
    expect(openAsByContent("README", big)).toBe("large-text");
    // The name wins over the bytes when it is definite.
    expect(openAsByContent("a.png", utf8("not really"))).toBe("image");
  });

  it("maps decisions to tab kinds", () => {
    expect(tabKindOf("text")).toBe("document");
    expect(tabKindOf("figure")).toBe("figure");
    for (const as of ["image", "pdf", "binary", "large-text"] as const) {
      expect(tabKindOf(as)).toBe("viewer");
    }
    expect(kindForPath("a.zip")).toBe("viewer");
    expect(kindForPath("README")).toBe("document");
    expect(kindForPath("README", "viewer")).toBe("viewer");
  });

  it("keeps the kind decided from bytes across opening and renaming", () => {
    let model = openTab(EMPTY_TABS, "blob", undefined, "viewer");
    expect(model.tabs[0]?.kind).toBe("viewer");
    model = renamePath(model, "blob", "blob2");
    expect(model.tabs[0]?.kind).toBe("viewer");
    // A large text file stays in its viewer when renamed to another text name.
    model = renamePath(
      openTab(EMPTY_TABS, "big.csv", undefined, "viewer"),
      "big.csv",
      "big.txt",
    );
    expect(model.tabs[0]?.kind).toBe("viewer");
  });

  it("describes files for the info tab", () => {
    expect(describeFile("a.zip")).toBe("ZIP archive");
    expect(describeFile("a.xyz")).toBe("XYZ file");
    expect(describeFile("blob")).toBe("Binary file");
    expect(describeFile("constructor")).toBe("Binary file");
    expect(formatSize(812)).toBe("812 B");
    expect(formatSize(3 * 1024 + 400)).toBe("3.4 KB");
    expect(formatSize(12 * 1024 * 1024)).toBe("12.0 MB");
    expect(formatModified(null)).toBe("");
    expect(formatModified(new Date(2026, 9, 18, 9, 5).getTime())).toBe(
      "2026-10-18 09:05",
    );
  });
});
