import { describe, expect, it } from "vitest";

import type { FigureNode } from "$lib/figure/scene";

import { figureApi, projectApi } from "./api";
import { DEMO_ROOT, createDevKernel } from "./dev";
import { figurePaths } from "./mockFigure";

const RECT: FigureNode = {
  kind: "rect",
  id: "r1",
  x: 10,
  y: 20,
  width: 40,
  height: 30,
  style: { stroke: "ink" },
};

async function opened() {
  const kernel = createDevKernel();
  await projectApi(kernel).open(DEMO_ROOT);
  return { kernel, figure: figureApi(kernel), project: projectApi(kernel) };
}

describe("browser figure plugin", () => {
  it("opens a project figure by absolute path and serves its scene", async () => {
    const { figure } = await opened();
    const { id } = await figure.open(`${DEMO_ROOT}/figures/state_lattice.figure`);
    const scene = await figure.get(id);
    expect(scene.nodes.length).toBe(4);
    await expect(figure.open("/elsewhere/x.figure.json")).rejects.toMatchObject({
      message: expect.stringContaining("outside the open project"),
    });
  });

  it("applies edits with revisions and history, like the Rust store", async () => {
    const { figure } = await opened();
    const { id } = await figure.createAt(`${DEMO_ROOT}/figures/t.figure.json`);
    expect(await figure.apply(id, { op: "add-node", node: RECT })).toEqual({
      revision: 1,
      applied: true,
      can_undo: true,
      can_redo: false,
      changed: ["r1"],
    });
    await figure.apply(id, { op: "translate", id: "r1", dx: 5, dy: -4 });
    expect((await figure.get(id)).nodes[0]).toMatchObject({ x: 15, y: 16, width: 40 });

    const undo = await figure.apply(id, { op: "undo" });
    expect(undo).toMatchObject({ revision: 3, applied: true, can_redo: true });
    expect((await figure.get(id)).nodes[0]).toMatchObject({ x: 10, y: 20 });

    const still = await figure.apply(id, { op: "translate", id: "r1", dx: 0, dy: 0 });
    expect(still).toMatchObject({ applied: false, can_redo: true });

    await expect(
      figure.apply(id, { op: "add-node", node: RECT }),
    ).rejects.toMatchObject({ message: expect.stringContaining("already exists") });
    await expect(
      figure.apply(id, { op: "remove-node", id: "nope" }),
    ).rejects.toMatchObject({ message: expect.stringContaining("not found") });
    expect((await figure.list())[0]).toMatchObject({
      revision: 3,
      nodes: 1,
      dirty: true,
    });

    expect((await figure.hitTest(id, 20, 30)).node_id).toBe("r1");
    expect((await figure.hitTest(id, 500, 500)).hit).toBe(false);
  });

  it("creates and saves the scene with its SVG next to it", async () => {
    const { figure, project } = await opened();
    const path = `${DEMO_ROOT}/figures/new.figure`;
    const created = await figure.createAt(path, 320, 200);
    expect(created).toMatchObject({ json: path, svg: `${DEMO_ROOT}/figures/new.svg` });
    expect(JSON.parse(await project.readFile("figures/new.figure"))).toMatchObject({
      width: 320,
      nodes: [],
    });
    await expect(figure.createAt(path)).rejects.toMatchObject({
      message: expect.stringContaining("already exists"),
    });

    await figure.apply(created.id, { op: "add-node", node: RECT });
    const renamed = `${DEMO_ROOT}/figures/renamed.figure.json`;
    const saved = await figure.save(created.id, renamed);
    expect(saved).toEqual({
      json: renamed,
      svg: `${DEMO_ROOT}/figures/renamed.svg`,
      revision: 1,
      pdf: null,
    });
    const svg = new TextDecoder().decode(
      await project.readBytes("figures/renamed.svg"),
    );
    expect(svg).toContain('id="r1"');
    expect((await figure.list()).find((f) => f.id === created.id)?.dirty).toBe(false);

    expect(new TextDecoder().decode(await figure.exportSvg(created.id))).toBe(svg);
    expect(await figure.close(created.id)).toEqual({ closed: true });
    expect(await figure.close(created.id)).toEqual({ closed: false });
    await expect(figure.get(created.id)).rejects.toBeTruthy();
  });

  it("serves project images as raw bytes", async () => {
    const { project } = await opened();
    const png = await project.readBytes("figures/sketch.png");
    // The PNG signature, untouched by any text encoding.
    expect([...png.slice(0, 8)]).toEqual([
      0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a,
    ]);
  });

  it("maps figure paths like resolve_figure_paths", () => {
    expect(figurePaths("/p/figures/a.figure.json")).toEqual({
      json: "/p/figures/a.figure.json",
      svg: "/p/figures/a.svg",
    });
    // A legacy name is kept; everything else is saved as `.figure`.
    expect(figurePaths("/p/figures/a.figure")).toEqual({
      json: "/p/figures/a.figure",
      svg: "/p/figures/a.svg",
    });
    expect(figurePaths("/p/a.svg").json).toBe("/p/a.figure");
    expect(figurePaths("/p/a.json").json).toBe("/p/a.figure");
    expect(figurePaths("/p/a").json).toBe("/p/a.figure");
    expect(figurePaths("/p/a").svg).toBe("/p/a.svg");
  });
});
