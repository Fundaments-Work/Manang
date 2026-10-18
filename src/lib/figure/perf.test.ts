/**
 * ADR 0012 budget: a pointer move over a 2,000-node scene (the drag preview
 * plus viewport culling) must fit well inside 8 ms. Measured as the mean of
 * many moves after a warm-up, with a generous bound so slow CI machines pass.
 */

import { describe, expect, it } from "vitest";

import { createScene, type FigureNode, type FigureScene } from "./scene";
import { planDrag, previewMove } from "./selection";
import { cull, visibleRect } from "./viewport";

function bigScene(n: number): FigureScene {
  const nodes: FigureNode[] = [];
  const shapes = Math.floor((n * 2) / 3);
  for (let i = 0; i < shapes; i += 1) {
    nodes.push({
      kind: i % 3 === 0 ? "ellipse" : "rect",
      id: `s${i}`,
      x: (i % 40) * 160,
      y: Math.floor(i / 40) * 100,
      width: 120,
      height: 60,
      label: `Node ${i}`,
    });
  }
  for (let i = 0; i < n - shapes; i += 1) {
    nodes.push({
      kind: "arrow",
      id: `c${i}`,
      points: [
        { x: 0, y: 0 },
        { x: 1, y: 1 },
      ],
      start: { node: `s${i}` },
      end: { node: `s${(i + 1) % shapes}` },
    });
  }
  return { ...createScene(4000, 4000), nodes };
}

describe("pointer-move budget", () => {
  it("previews a drag and culls a 2,000-node scene well under 8 ms", () => {
    const scene = bigScene(2000);
    // A selection of 20 shapes, each with bound arrows.
    const ids = Array.from({ length: 20 }, (_, i) => `s${i * 7}`);
    const plan = planDrag(scene, ids);
    const view = visibleRect({ x: 0, y: 0, zoom: 0.5 }, 1600, 1000, 200);
    const frame = (k: number) =>
      cull(previewMove(scene, plan, { x: k, y: k / 2 }).nodes, view);
    for (let k = 0; k < 10; k += 1) frame(k);
    const runs = 60;
    const t0 = performance.now();
    let shown = 0;
    for (let k = 0; k < runs; k += 1) shown = frame(k + 10).length;
    const perMove = (performance.now() - t0) / runs;
    console.warn(
      `figure pointer move (2,000 nodes, 20 selected): ${perMove.toFixed(3)} ms`,
    );
    expect(shown).toBeGreaterThan(0);
    expect(perMove).toBeLessThan(8);
  });
});
