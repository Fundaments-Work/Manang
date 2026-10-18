import { describe, expect, it } from "vitest";

import { latexApi, projectApi, typstApi } from "./api";
import { createDevKernel, DEMO_PROJECTS, DEMO_ROOT } from "./dev";
import { createMockKernel } from "./mock";
import { latexPlugin } from "./mockLatex";
import { MockProjectHost, projectPlugin } from "./mockProject";

const THESIS = "/home/author/thesis-draft";

describe("mock typst jumps", () => {
  it("maps a cursor to its page and a click there back to the line", async () => {
    const kernel = createDevKernel();
    await projectApi(kernel).open(DEMO_ROOT);
    const typst = typstApi(kernel);
    const report = await typst.compile({ main: "main.typ", root: DEMO_ROOT });
    expect(report.ok).toBe(true);

    const text = DEMO_PROJECTS[0]?.files["main.typ"];
    expect(typeof text).toBe("string");
    const lines = String(text).split("\n");
    const heading = lines.findIndex((l) => /^=\s/.test(l)) + 1;
    expect(heading).toBeGreaterThan(0);

    const points = await typst.jumpFromCursor("main.typ", heading, 3);
    expect(points).toHaveLength(1);
    const [point] = points;
    if (point === undefined) throw new Error("no point");
    const jump = await typst.jumpFromClick(point.page, point.x + 5, point.y - 4);
    expect(jump).toEqual({ kind: "file", path: "main.typ", line: heading, column: 1 });
  });

  it("finds nothing in the margin and rejects bad arguments", async () => {
    const kernel = createDevKernel();
    await projectApi(kernel).open(DEMO_ROOT);
    const typst = typstApi(kernel);
    await expect(typst.jumpFromClick(0, 10, 10)).rejects.toBeDefined();
    await typst.compile({ main: "main.typ", root: DEMO_ROOT });
    expect(await typst.jumpFromClick(0, 2, 300)).toBeNull();
    await expect(typst.jumpFromClick(99, 100, 100)).rejects.toBeDefined();
    await expect(typst.jumpFromClick(0, Number.NaN, 1)).rejects.toBeDefined();
    expect(await typst.jumpFromCursor("nowhere.typ", 1, 1)).toEqual([]);
  });
});

describe("mock latex synctex", () => {
  it("round-trips a line of an \\input file", async () => {
    const kernel = createDevKernel();
    await projectApi(kernel).open(THESIS);
    const latex = latexApi(kernel);
    const main = `${THESIS}/main.tex`;
    expect((await latex.compile({ main })).ok).toBe(true);

    const forward = await latex.synctexForward(main, "chapters/methods.tex", 2);
    const box = forward.positions[0];
    if (box === undefined) throw new Error("no position");
    expect(box.page).toBe(1);
    const back = await latex.synctexReverse(main, box.page, box.x + 2, box.y + 5);
    expect(back).toMatchObject({ file: "chapters/methods.tex", line: 2 });

    const intro = await latex.synctexForward(main, "main.tex", 5);
    const at = intro.positions[0];
    if (at === undefined) throw new Error("no position");
    expect(await latex.synctexReverse(main, 1, at.x, at.y + 5)).toMatchObject({
      file: "main.tex",
      line: 5,
    });
    await expect(latex.synctexForward(main, "other.tex", 1)).rejects.toBeDefined();
  });

  it("reports whether TeX is available", async () => {
    const kernel = createDevKernel();
    expect((await latexApi(kernel).status()).default).toBe("system");

    const host = new MockProjectHost(DEMO_PROJECTS, []);
    const bare = createMockKernel({
      plugins: [projectPlugin(host), latexPlugin(host, { tex: false })],
    });
    expect((await latexApi(bare).status()).default).toBe("none");
  });
});
