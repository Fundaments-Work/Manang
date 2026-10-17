import { describe, expect, it } from "vitest";

import { projectApi, typstApi } from "./api";
import { sniffArtifact } from "./artifact";
import { DEMO_ROOT, createDevKernel } from "./dev";
import type { KernelEvent } from "./client";

async function opened() {
  const kernel = createDevKernel();
  const project = projectApi(kernel);
  const typst = typstApi(kernel);
  await project.open(DEMO_ROOT);
  return { kernel, project, typst };
}

describe("dev kernel: project plugin", () => {
  it("contributes the commands of both manifests", async () => {
    const ids = (await createDevKernel().commands()).map((c) => c.id);
    expect(ids).toContain("project.git-status");
    expect(ids).toContain("typst.export-pdf");
    expect(ids).toContain("editor.new");
    expect(ids).toEqual([...ids].sort());
  });

  it("refuses commands until a project is open", async () => {
    const project = projectApi(createDevKernel());
    await expect(project.tree()).rejects.toMatchObject({
      message: "no project is open",
    });
  });

  it("opens a project and answers with the Rust summary shape", async () => {
    const { project } = await opened();
    const again = await project.open(DEMO_ROOT);
    expect(again).toMatchObject({
      root: DEMO_ROOT,
      engine: "typst",
      main: "main.typ",
      config: { project: { engine: "typst" } },
    });
    expect(again.tree.entries.map((e) => e.name)).toEqual(
      expect.arrayContaining(["main.typ", "figures", "sections"]),
    );
  });

  it("rejects folders without a manang.toml with a message that says so", async () => {
    const project = projectApi(createDevKernel());
    await expect(project.open("/nope")).rejects.toMatchObject({
      message: expect.stringContaining("not a Manang project"),
    });
  });

  it("lists one directory at a time and pages recursive listings", async () => {
    const { project } = await opened();
    const figures = await project.tree({ path: "figures" });
    expect(figures.entries.map((e) => e.path)).toEqual([
      "figures/dissipation_curve.figure.json",
      "figures/state_lattice.figure.json",
      "figures/state_lattice.svg",
    ]);
    const page = await project.tree({ recursive: true, limit: 3 });
    expect(page.entries).toHaveLength(3);
    expect(page.has_more).toBe(true);
    expect(page.total).toBeGreaterThan(3);
  });

  it("confines paths to the project", async () => {
    const { project } = await opened();
    await expect(project.readFile("../secret")).rejects.toMatchObject({
      message: expect.stringContaining("escapes the project"),
    });
  });

  it("writes files, reports git changes and publishes project.changed", async () => {
    const { kernel, project } = await opened();
    const events: KernelEvent[] = [];
    await kernel.onEvent((e) => events.push(e));

    await project.writeFile("main.typ", "= Changed\n");
    await project.writeFile("sections/new.typ", "== New\n");
    await new Promise((resolve) => setTimeout(resolve, 5));

    expect(await project.readFile("main.typ")).toBe("= Changed\n");
    const status = await project.gitStatus();
    const byPath = Object.fromEntries(status.changes.map((c) => [c.path, c.unstaged]));
    expect(byPath["main.typ"]).toBe("modified");
    expect(byPath["sections/new.typ"]).toBe("untracked");
    expect(status.branch).toBe("main");
    expect(events.map((e) => e.topic)).toEqual(["project.changed", "project.changed"]);
  });

  it("renames files and folders, and refuses to overwrite", async () => {
    const { project } = await opened();
    await project.rename("notes.md", "scratch.md");
    await expect(project.readFile("notes.md")).rejects.toBeDefined();
    await project.rename("figures", "plots");
    expect((await project.tree({ path: "plots" })).entries).toHaveLength(3);
    await expect(project.rename("scratch.md", "main.typ")).rejects.toMatchObject({
      message: "main.typ: already exists",
    });
  });

  it("moves files to the trash and reports where", async () => {
    const { project } = await opened();
    const result = await project.trash("notes.md");
    expect(result.trashed_to).toContain("notes.md");
    await expect(project.readFile("notes.md")).rejects.toBeDefined();
    expect(
      (await project.gitStatus()).changes.find((c) => c.path === "notes.md"),
    ).toBeUndefined();
  });

  it("creates a project and lists it first among recents", async () => {
    const kernel = createDevKernel();
    const project = projectApi(kernel);
    const created = await project.create(
      "/home/author/new-paper",
      "New paper",
      "latex",
    );
    expect(created.main).toBe("main.tex");
    const recent = await project.recent();
    expect(recent[0]).toMatchObject({ path: "/home/author/new-paper", exists: true });
    expect(recent.some((r) => !r.exists)).toBe(true);
  });
});

describe("dev kernel: typst plugin", () => {
  it("compiles the demo paper cleanly, with citations resolved from the .bib", async () => {
    const { typst } = await opened();
    const report = await typst.compile({ main: "main.typ" });
    expect(report.diagnostics).toEqual([]);
    expect(report.ok).toBe(true);
    expect(report.pages).toBeGreaterThanOrEqual(1);
    expect(report.duration_ms).toBeGreaterThan(0);
  });

  it("reports an unclosed formula with a 1-based line and column", async () => {
    const { typst } = await opened();
    const report = await typst.compile({
      main: "main.typ",
      sources: { "main.typ": "= Title\n\nSome text $x + y\n" },
    });
    expect(report.ok).toBe(false);
    expect(report.diagnostics[0]).toMatchObject({
      severity: "error",
      message: "Unclosed math delimiter",
      file: "/main.typ",
      line: 3,
    });
    expect(report.diagnostics[0]?.column).toBeGreaterThan(1);
    expect(report.diagnostics[0]?.range?.end).toBeGreaterThan(
      report.diagnostics[0]?.range?.start ?? 0,
    );
  });

  it("names a missing include and unknown citation, in the file they occur in", async () => {
    const { typst } = await opened();
    const report = await typst.compile({
      main: "main.typ",
      sources: { "main.typ": '= T\n\n#include "sections/nope.typ"\n' },
    });
    expect(report.diagnostics.map((d) => d.message)).toContain(
      "file not found (searched at /sections/nope.typ)",
    );
    const missingEntry = await typst.compile({
      main: "main.typ",
      sources: {
        "main.typ": '#include "sections/results.typ"\n',
        "sections/results.typ": "see @nobody2020\n",
      },
    });
    expect(missingEntry.diagnostics[0]).toMatchObject({
      severity: "warning",
      file: "/sections/results.typ",
      line: 1,
    });
  });

  it("renders compiled pages as SVG bytes and exports a PDF", async () => {
    const { typst } = await opened();
    await typst.compile({ main: "main.typ" });
    const page = await typst.render(0);
    expect(page).toBeInstanceOf(Uint8Array);
    expect(sniffArtifact(page)?.kind).toBe("svg");
    expect(new TextDecoder().decode(page)).toContain("Quantum thermodynamic bounds");
    expect(sniffArtifact(await typst.exportPdf())?.kind).toBe("pdf");
  });

  it("refuses to render before a successful compile or past the last page", async () => {
    const { typst } = await opened();
    await expect(typst.render(0)).rejects.toMatchObject({
      message: expect.stringContaining("nothing has been compiled"),
    });
    await typst.compile({ main: "main.typ" });
    await expect(typst.render(99)).rejects.toMatchObject({
      message: expect.stringContaining("page 100 does not exist"),
    });
  });

  it("keeps the last good document when a compile fails", async () => {
    const { typst } = await opened();
    await typst.compile({ main: "main.typ" });
    const failed = await typst.compile({
      main: "main.typ",
      sources: { "main.typ": "$oops" },
    });
    expect(failed.ok).toBe(false);
    expect(failed.pages).toBeGreaterThanOrEqual(1);
    await expect(typst.render(0)).resolves.toBeInstanceOf(Uint8Array);
  });
});
