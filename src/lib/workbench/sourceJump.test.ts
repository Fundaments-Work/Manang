import { describe, expect, it } from "vitest";

import { latexSourcePath, svgPageSize, synctexFile, typstMarkBox } from "./sourceJump";

describe("source and preview helpers", () => {
  it("resolves SyncTeX files against the main file's directory", () => {
    expect(latexSourcePath("chapters/a.tex", "main.tex", "/p")).toBe("chapters/a.tex");
    expect(latexSourcePath("a.tex", "thesis/main.tex", "/p")).toBe("thesis/a.tex");
    expect(latexSourcePath("./a.tex", "thesis/main.tex", "/p")).toBe("thesis/a.tex");
    expect(latexSourcePath("../b.tex", "thesis/main.tex", "/p")).toBe("b.tex");
    expect(latexSourcePath("../../x.tex", "thesis/main.tex", "/p")).toBeNull();
    expect(latexSourcePath("/p/shared/m.tex", "thesis/main.tex", "/p/")).toBe(
      "shared/m.tex",
    );
    expect(
      latexSourcePath("/usr/share/texmf/article.cls", "main.tex", "/p"),
    ).toBeNull();
    expect(latexSourcePath("/pq/a.tex", "main.tex", "/p")).toBeNull();
  });

  it("names project files the way SyncTeX does", () => {
    expect(synctexFile("chapters/a.tex", "main.tex", "/p")).toBe("chapters/a.tex");
    expect(synctexFile("thesis/a.tex", "thesis/main.tex", "/p")).toBe("a.tex");
    expect(synctexFile("shared/m.tex", "thesis/main.tex", "/p")).toBe(
      "/p/shared/m.tex",
    );
  });

  it("marks a band above a Typst baseline", () => {
    const box = typstMarkBox({ page: 1, x: 72, y: 100 }, 595);
    expect(box.page).toBe(1);
    expect(box.x).toBe(70);
    expect(box.y).toBeLessThan(100);
    expect(box.y + box.height).toBeGreaterThan(100);
    expect(box.x + box.width).toBeLessThanOrEqual(595);
    expect(typstMarkBox({ page: 0, x: 0, y: 2 }, null).y).toBe(0);
  });

  it("reads the page size from an SVG viewBox", () => {
    const svg = (head: string) => new TextEncoder().encode(`${head}<rect/></svg>`);
    expect(
      svgPageSize(svg('<svg class="typst-doc" viewBox="0 0 200 100.5" width="200pt">')),
    ).toEqual({ w: 200, h: 100.5 });
    expect(svgPageSize(svg("<svg>"))).toBeNull();
  });
});
