import { describe, expect, it } from "vitest";

import {
  AGENT_MAX,
  AGENT_MIN,
  DEFAULT_LAYOUT,
  agentOverlays,
  PANEL_MAX,
  SIDE_MAX,
  SIDE_MIN,
  SPLIT_MAX,
  cyclePreview,
  effectivePreview,
  isNarrow,
  revealPreview,
  sanitizeLayout,
  showPanel,
  toggleMaximisedPreview,
  toggleSide,
} from "./layout";

describe("layout model", () => {
  it("collapses the side panel when the active activity icon is clicked again", () => {
    const open = toggleSide(DEFAULT_LAYOUT, "search");
    expect(open.sideView).toBe("search");
    expect(toggleSide(open, "search").sideView).toBeNull();
    expect(toggleSide(toggleSide(open, "search"), "files").sideView).toBe("files");
  });

  it("opens the bottom panel on a given tab", () => {
    expect(showPanel(DEFAULT_LAYOUT, "output")).toMatchObject({
      panelOpen: true,
      panelTab: "output",
    });
  });

  it("keeps the Agent column's width within bounds and drops the old bottom tab", () => {
    expect(sanitizeLayout({ agentWidth: 10, agentOpen: true })).toMatchObject({
      agentWidth: AGENT_MIN,
      agentOpen: true,
    });
    expect(sanitizeLayout({ agentWidth: 9000 }).agentWidth).toBe(AGENT_MAX);
    // An older build stored the Agent as a bottom-panel tab.
    expect(sanitizeLayout({ panelTab: "agent" }).panelTab).toBe("problems");
    expect(DEFAULT_LAYOUT.agentOpen).toBe(false);
  });

  it("floats the Agent column over the editor under 900px", () => {
    expect(agentOverlays(899)).toBe(true);
    expect(agentOverlays(900)).toBe(false);
  });

  it("cycles preview split, editor, preview", () => {
    expect(cyclePreview("split")).toBe("editor");
    expect(cyclePreview("editor")).toBe("preview");
    expect(cyclePreview("preview")).toBe("split");
  });

  it("drops split to the editor under 800px but keeps an explicit preview", () => {
    expect(isNarrow(799)).toBe(true);
    expect(isNarrow(800)).toBe(false);
    expect(effectivePreview("split", 700)).toBe("editor");
    expect(effectivePreview("split", 1000)).toBe("split");
    expect(effectivePreview("preview", 700)).toBe("preview");
  });

  it("maximises the preview from split and restores split from preview only", () => {
    expect(toggleMaximisedPreview("split")).toBe("preview");
    expect(toggleMaximisedPreview("preview")).toBe("split");
  });

  it("shows the preview again in split, or alone when split cannot fit", () => {
    expect(revealPreview(1280)).toBe("split");
    expect(revealPreview(800)).toBe("split");
    expect(revealPreview(640)).toBe("preview");
    // What comes back is visible: the narrow rule never hides it again.
    expect(effectivePreview(revealPreview(640), 640)).toBe("preview");
    expect(effectivePreview(revealPreview(1280), 1280)).toBe("split");
  });
});

describe("sanitizeLayout", () => {
  it("returns defaults for junk", () => {
    expect(sanitizeLayout(null)).toEqual(DEFAULT_LAYOUT);
    expect(sanitizeLayout("x")).toEqual(DEFAULT_LAYOUT);
    expect(sanitizeLayout({ sideView: "nope", preview: 3, theme: "pink" })).toEqual(
      DEFAULT_LAYOUT,
    );
  });

  it("clamps persisted sizes into range", () => {
    const layout = sanitizeLayout({
      sideWidth: 9999,
      panelHeight: 99999,
      splitRatio: 5,
    });
    expect(layout.sideWidth).toBe(SIDE_MAX);
    expect(layout.panelHeight).toBe(PANEL_MAX);
    expect(layout.splitRatio).toBe(SPLIT_MAX);
    expect(sanitizeLayout({ sideWidth: 1 }).sideWidth).toBe(SIDE_MIN);
    expect(sanitizeLayout({ sideWidth: Number.NaN }).sideWidth).toBe(
      DEFAULT_LAYOUT.sideWidth,
    );
  });

  it("keeps a collapsed side panel collapsed", () => {
    expect(sanitizeLayout({ sideView: null }).sideView).toBeNull();
    expect(
      sanitizeLayout({ sideView: "outline", panelOpen: true, theme: "light" }),
    ).toMatchObject({ sideView: "outline", panelOpen: true, theme: "light" });
  });

  it("keeps the Plugins side view but drops views that are pages (settings)", () => {
    expect(sanitizeLayout({ sideView: "plugins" }).sideView).toBe("plugins");
    expect(sanitizeLayout({ sideView: "settings" }).sideView).toBe("files");
  });
});
