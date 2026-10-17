/**
 * Workbench layout model: pure data and the rules that keep it valid.
 *
 * The runes store (`state/layout.svelte.ts`) wraps these functions; they are
 * kept free of reactivity so the rules are unit-testable and so persisted data
 * from an older build can be validated before it reaches the UI.
 */

export type SideView =
  | "files"
  | "search"
  | "source-control"
  | "figures"
  | "references"
  | "plugins"
  | "settings";
export type PanelTab = "problems" | "output";
export type PreviewMode = "split" | "editor" | "preview";
export type Theme = "dark" | "light";

export const SIDE_VIEWS: readonly SideView[] = [
  "files",
  "search",
  "source-control",
  "figures",
  "references",
  "plugins",
  "settings",
];

export const SIDE_VIEW_ICONS: Record<SideView, string> = {
  files: "files",
  search: "search",
  "source-control": "source-control",
  figures: "figures",
  references: "references",
  plugins: "plugins",
  settings: "settings",
};

export interface LayoutModel {
  /** The open side panel, or null when collapsed. */
  readonly sideView: SideView | null;
  readonly sideWidth: number;
  readonly panelOpen: boolean;
  readonly panelHeight: number;
  readonly panelTab: PanelTab;
  readonly preview: PreviewMode;
  /** Fraction of the editor+preview area given to the editor. */
  readonly splitRatio: number;
  readonly theme: Theme;
}

export const SIDE_MIN = 200;
export const SIDE_MAX = 480;
export const PANEL_MIN = 96;
export const PANEL_MAX = 560;
export const SPLIT_MIN = 0.25;
export const SPLIT_MAX = 0.75;
/** Below this viewport width the side panel overlays the editor. */
export const NARROW_BELOW = 800;
export const MIN_WIDTH = 640;
export const ACTIVITY_WIDTH = 48;

export const DEFAULT_LAYOUT: LayoutModel = {
  sideView: "files",
  sideWidth: 264,
  panelOpen: false,
  panelHeight: 200,
  panelTab: "problems",
  preview: "split",
  splitRatio: 0.5,
  theme: "dark",
};

export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

export const clampSide = (width: number): number =>
  clamp(Math.round(width), SIDE_MIN, SIDE_MAX);
export const clampPanel = (height: number): number =>
  clamp(Math.round(height), PANEL_MIN, PANEL_MAX);
export const clampSplit = (ratio: number): number => clamp(ratio, SPLIT_MIN, SPLIT_MAX);

export function isNarrow(viewportWidth: number): boolean {
  return viewportWidth < NARROW_BELOW;
}

/**
 * Split needs room for two readable columns. When the window is narrow it
 * degrades to the editor alone; an explicit "preview" choice is kept.
 */
export function effectivePreview(
  mode: PreviewMode,
  viewportWidth: number,
): PreviewMode {
  return mode === "split" && isNarrow(viewportWidth) ? "editor" : mode;
}

/** Clicking the active activity icon collapses its panel; any other opens it. */
export function toggleSide(model: LayoutModel, view: SideView): LayoutModel {
  return { ...model, sideView: model.sideView === view ? null : view };
}

export function showSide(model: LayoutModel, view: SideView): LayoutModel {
  return { ...model, sideView: view };
}

export function togglePanel(model: LayoutModel): LayoutModel {
  return { ...model, panelOpen: !model.panelOpen };
}

export function showPanel(model: LayoutModel, tab: PanelTab): LayoutModel {
  return { ...model, panelOpen: true, panelTab: tab };
}

const PREVIEW_CYCLE: readonly PreviewMode[] = ["split", "editor", "preview"];

export function cyclePreview(mode: PreviewMode): PreviewMode {
  const at = PREVIEW_CYCLE.indexOf(mode);
  return PREVIEW_CYCLE[(at + 1) % PREVIEW_CYCLE.length] ?? "split";
}

/**
 * Whether the side panel for `view` can be shown. Panels that browse the
 * project need one open; Plugins and Settings work without.
 */
export function sidePanelVisible(
  view: SideView | null,
  hasProject: boolean,
): view is SideView {
  return view !== null && (hasProject || view === "plugins" || view === "settings");
}

function isOneOf<T extends string>(value: unknown, options: readonly T[]): value is T {
  return typeof value === "string" && (options as readonly string[]).includes(value);
}

function finite(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

/** Validate persisted layout. Anything malformed falls back to its default. */
export function sanitizeLayout(raw: unknown): LayoutModel {
  const d = DEFAULT_LAYOUT;
  if (typeof raw !== "object" || raw === null) return d;
  const r = raw as Record<string, unknown>;
  return {
    sideView:
      r["sideView"] === null
        ? null
        : isOneOf(r["sideView"], SIDE_VIEWS)
          ? r["sideView"]
          : d.sideView,
    sideWidth: finite(r["sideWidth"]) ? clampSide(r["sideWidth"]) : d.sideWidth,
    panelOpen: typeof r["panelOpen"] === "boolean" ? r["panelOpen"] : d.panelOpen,
    panelHeight: finite(r["panelHeight"])
      ? clampPanel(r["panelHeight"])
      : d.panelHeight,
    panelTab: isOneOf(r["panelTab"], ["problems", "output"] as const)
      ? r["panelTab"]
      : d.panelTab,
    preview: isOneOf(r["preview"], PREVIEW_CYCLE) ? r["preview"] : d.preview,
    splitRatio: finite(r["splitRatio"]) ? clampSplit(r["splitRatio"]) : d.splitRatio,
    theme: isOneOf(r["theme"], ["dark", "light"] as const) ? r["theme"] : d.theme,
  };
}
