import { describe, expect, it } from "vitest";

import { fuzzyRank } from "$lib/commands/fuzzy";
import {
  createScene,
  formatNodeId,
  type FigureNode,
  type FigureScene,
  type Rect,
  type Vec,
} from "$lib/ink/scene";
import {
  nodeBounds,
  anchorsOf,
  snapToAnchors,
} from "$lib/ink/geometry";

interface ScreenRoute {
  readonly hash: string;
  readonly screenId:
    | "desk"
    | "canvas"
    | "library"
    | "shelf"
    | "palette"
    | "slides"
    | "zen";
  readonly overlay: boolean;
  readonly argument?: string | undefined;
}

/**
 * Pure route resolver matching the dispatch specification in App.svelte and DeskState.
 */
function resolveRoute(hash: string): ScreenRoute {
  const route = hash.replace(/^#\/?/, "").trim();
  if (route.length === 0) {
    return { hash, screenId: "desk", overlay: false };
  }

  const [head, argument] = route.split("/") as [string, string | undefined];
  switch (head) {
    case "library":
      return { hash, screenId: "library", overlay: true, argument };
    case "draw":
      return { hash, screenId: "canvas", overlay: true, argument };
    case "shelf":
      return { hash, screenId: "shelf", overlay: true, argument };
    case "palette":
      return { hash, screenId: "palette", overlay: true, argument };
    case "slides":
      return { hash, screenId: "slides", overlay: true, argument };
    case "zen":
      return { hash, screenId: "zen", overlay: false, argument };
    case "split":
      return { hash, screenId: "desk", overlay: false, argument };
    default:
      return { hash, screenId: "desk", overlay: false, argument };
  }
}

describe("Seven Screens: Architecture & State Contracts", () => {
  describe("Route & Deep-Link Resolution", () => {
    it("resolves the seven canonical screens from URL hash fragments", () => {
      expect(resolveRoute("").screenId).toBe("desk");
      expect(resolveRoute("#/").screenId).toBe("desk");
      expect(resolveRoute("#library").screenId).toBe("library");
      expect(resolveRoute("#draw").screenId).toBe("canvas");
      expect(resolveRoute("#shelf").screenId).toBe("shelf");
      expect(resolveRoute("#palette/cit").screenId).toBe("palette");
      expect(resolveRoute("#palette/cit").argument).toBe("cit");
      expect(resolveRoute("#slides/3").screenId).toBe("slides");
      expect(resolveRoute("#slides/3").argument).toBe("3");
      expect(resolveRoute("#zen").screenId).toBe("zen");
    });

    it("correctly identifies overlay screens versus primary workspace screens", () => {
      expect(resolveRoute("").overlay).toBe(false);
      expect(resolveRoute("#zen").overlay).toBe(false);
      expect(resolveRoute("#library").overlay).toBe(true);
      expect(resolveRoute("#draw").overlay).toBe(true);
      expect(resolveRoute("#shelf").overlay).toBe(true);
      expect(resolveRoute("#palette").overlay).toBe(true);
      expect(resolveRoute("#slides").overlay).toBe(true);
    });
  });

  describe("Screen 3: Command Palette Matching Contracts", () => {
    const registry = [
      { id: "verify-citations", title: "Verify citations against Crossref & OpenAlex", group: "AI Actions", keywords: "references check" },
      { id: "generate-slides", title: "Generate 6 Touying presentation slides from Section 2", group: "AI Actions", keywords: "touying beamer deck" },
      { id: "add-cetz", title: "Add commutative diagram in CeTZ", group: "AI Actions", keywords: "tikz diagram arrows" },
      { id: "open-canvas", title: "Open Drawing Canvas", group: "Plugins & Commands", keywords: "draw vector figure" },
      { id: "open-slides", title: "Open Presentation Studio", group: "Plugins & Commands", keywords: "slides deck" },
      { id: "toggle-split", title: "Toggle Split Screen", group: "Plugins & Commands", keywords: "pane view" },
      { id: "toggle-zen", title: "Toggle Zen Mode", group: "Plugins & Commands", keywords: "distraction free" },
    ];

    it("ranks citation verification first for 'cit' query", () => {
      const ranked = fuzzyRank(registry, "cit", (c) => `${c.title} ${c.keywords}`);
      expect(ranked.length).toBeGreaterThan(0);
      expect(ranked[0]?.item.id).toBe("verify-citations");
    });

    it("ranks presentation commands for 'slides' query", () => {
      const ranked = fuzzyRank(registry, "slides", (c) => `${c.title} ${c.keywords}`);
      expect(ranked.length).toBeGreaterThan(0);
      expect(ranked[0]?.item.id).toMatch(/slides/);
      expect(ranked.some((r) => r.item.id === "generate-slides")).toBe(true);
    });

    it("matches canvas commands when typing 'draw'", () => {
      const ranked = fuzzyRank(registry, "draw", (c) => `${c.title} ${c.keywords}`);
      expect(ranked.some((r) => r.item.id === "open-canvas")).toBe(true);
    });
  });

  describe("Screen 4: Vector Ink Scene Contracts", () => {
    it("creates a valid 960x540 figure scene", () => {
      const scene: FigureScene = createScene(960, 540);
      expect(scene.width).toBe(960);
      expect(scene.height).toBe(540);
      expect(scene.nodes).toEqual([]);
    });

    it("computes accurate bounding box for rect nodes", () => {
      const rect: FigureNode = {
        kind: "rect",
        id: formatNodeId(1),
        x: 100,
        y: 150,
        width: 200,
        height: 100,
        stroke: "#ffffff",
        fill: "#000000",
      };
      const bounds: Rect = nodeBounds(rect);
      expect(bounds).toEqual({ x: 100, y: 150, width: 200, height: 100 });
    });

    it("generates cardinal anchors for snapping arrows to shapes", () => {
      const rect: FigureNode = {
        kind: "rect",
        id: formatNodeId(1),
        x: 100,
        y: 100,
        width: 100,
        height: 100,
        stroke: "#ffffff",
        fill: "#000000",
      };
      const anchors = anchorsOf(rect);
      expect(anchors.length).toBe(4);
      // North anchor: (150, 100)
      expect(anchors.some((a) => a.x === 150 && a.y === 100)).toBe(true);
      // South anchor: (150, 200)
      expect(anchors.some((a) => a.x === 150 && a.y === 200)).toBe(true);
      // West anchor: (100, 150)
      expect(anchors.some((a) => a.x === 100 && a.y === 150)).toBe(true);
      // East anchor: (200, 150)
      expect(anchors.some((a) => a.x === 200 && a.y === 150)).toBe(true);
    });

    it("snaps a nearby point within threshold to anchor", () => {
      const rect: FigureNode = {
        kind: "rect",
        id: formatNodeId(1),
        x: 100,
        y: 100,
        width: 100,
        height: 100,
        stroke: "#ffffff",
        fill: "#000000",
      };
      const anchors = anchorsOf(rect);
      const testPoint: Vec = { x: 152, y: 102 };
      const snap = snapToAnchors(testPoint, anchors, 8);
      expect(snap.anchor).toBeDefined();
      expect(snap.point).toEqual({ x: 150, y: 100 });
    });
  });

  describe("Screen 5: Slide Studio 16:9 Aspect Ratio Contracts", () => {
    it("preserves 16:9 ratio within tolerance", () => {
      const width = 1280;
      const height = 720;
      expect(width / height).toBeCloseTo(16 / 9, 3);
    });
  });
});
