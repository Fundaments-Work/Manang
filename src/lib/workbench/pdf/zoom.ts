/**
 * Zoom for the PDF viewer, Overleaf style: fit width, fit page, or a
 * percentage where 100% is the page at its printed size on a 96 dpi screen
 * (one point is 4/3 CSS px, as in pdf.js and every browser PDF viewer).
 *
 * Pure: the viewer turns a mode into a scale (CSS px per PDF point) for the
 * pane it is in, and steps, wheels and keys turn a scale into the next mode.
 */

/** `page-width` and `page-fit` follow the pane; a number is a zoom factor (1 = 100%). */
export type ZoomMode = "page-width" | "page-fit" | number;

/** CSS px per PDF point at 100%. */
export const CSS_PER_POINT = 96 / 72;
export const MIN_ZOOM = 0.25;
export const MAX_ZOOM = 4;
/** The fixed steps of the zoom menu and of the −/+ buttons. */
export const ZOOM_PRESETS: readonly number[] = [0.5, 0.75, 1, 1.25, 1.5, 2, 3, 4];

/** Space around the pages inside the scroll area, in CSS px. */
export interface Margins {
  /** Left and right of the column of pages. */
  readonly side: number;
  /** Above, between and below pages. */
  readonly gap: number;
}

export const MARGINS: Margins = { side: 24, gap: 16 };

export interface Size {
  readonly width: number;
  readonly height: number;
}

/** A page's size in points. */
export interface PageSize {
  readonly w: number;
  readonly h: number;
}

export function clampZoom(zoom: number): number {
  if (!Number.isFinite(zoom)) return 1;
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom));
}

/**
 * CSS px per point for `mode` in a pane of `pane` size showing pages as wide
 * as `page` (the widest page). Fit modes are not clamped to the preset range
 * (a tiny pane still shows the whole width), only kept positive.
 */
export function scaleFor(
  mode: ZoomMode,
  pane: Size,
  page: PageSize,
  margins: Margins = MARGINS,
): number {
  if (typeof mode === "number") return clampZoom(mode) * CSS_PER_POINT;
  const w = Math.max(1, page.w);
  const h = Math.max(1, page.h);
  const width = Math.max(40, pane.width - margins.side * 2) / w;
  if (mode === "page-width") return width;
  const height = Math.max(40, pane.height - margins.gap * 2) / h;
  return Math.min(width, height);
}

/** The zoom factor a scale stands for (1 = 100%). */
export function zoomOf(scale: number): number {
  return scale / CSS_PER_POINT;
}

/** "125%" for the header. */
export function zoomPercent(scale: number): string {
  return `${Math.round(zoomOf(scale) * 100)}%`;
}

/** Spoken label of the current zoom. */
export function zoomLabel(mode: ZoomMode, scale: number): string {
  if (mode === "page-width") return `Fit width, ${zoomPercent(scale)}`;
  if (mode === "page-fit") return `Fit page, ${zoomPercent(scale)}`;
  return zoomPercent(scale);
}

/**
 * The next preset above (`direction` 1) or below (-1) the current scale,
 * whatever mode produced it; stays at the end of the range.
 */
export function stepZoom(scale: number, direction: 1 | -1): number {
  const current = zoomOf(scale);
  const next =
    direction === 1
      ? ZOOM_PRESETS.find((z) => z > current * 1.001)
      : [...ZOOM_PRESETS].reverse().find((z) => z < current * 0.999);
  return next ?? (direction === 1 ? MAX_ZOOM : MIN_ZOOM);
}

/** Wheel deltas arrive in pixels, lines or pages depending on the device. */
const DELTA_PX: Readonly<Record<number, number>> = { 0: 1, 1: 16, 2: 400 };

/**
 * Zoom factor after a Ctrl+wheel (or trackpad pinch, which browsers report as
 * a Ctrl+wheel with small deltas): smooth and proportional, so a pinch feels
 * continuous and a notched wheel moves about 20% per notch.
 */
export function wheelZoom(scale: number, deltaY: number, deltaMode = 0): number {
  const px = deltaY * (DELTA_PX[deltaMode] ?? 1);
  // A notch is about 100 px; clamp so one huge event cannot jump the range.
  const step = Math.max(-300, Math.min(300, px));
  return clampZoom(zoomOf(scale) * Math.exp(-step * 0.0018));
}

export type ZoomKey = "in" | "out" | "fit";

/** Ctrl+= / Ctrl++ / Ctrl+− / Ctrl+0 (Cmd on macOS), or null. */
export function zoomKey(event: {
  readonly key: string;
  readonly ctrlKey: boolean;
  readonly metaKey: boolean;
  readonly altKey: boolean;
}): ZoomKey | null {
  if (!(event.ctrlKey || event.metaKey) || event.altKey) return null;
  switch (event.key) {
    case "=":
    case "+":
      return "in";
    case "-":
    case "_":
      return "out";
    case "0":
      return "fit";
    default:
      return null;
  }
}

/** The entries of the zoom menu, in order. */
export const ZOOM_MENU: readonly { readonly label: string; readonly mode: ZoomMode }[] =
  [
    { label: "Fit width", mode: "page-width" },
    { label: "Fit page", mode: "page-fit" },
    ...ZOOM_PRESETS.map((z) => ({ label: `${Math.round(z * 100)}%`, mode: z })),
  ];

/** Whether a menu entry is the current mode (a number matches within 0.5%). */
export function isCurrentMode(entry: ZoomMode, mode: ZoomMode): boolean {
  if (typeof entry === "number" && typeof mode === "number")
    return Math.abs(entry - mode) < 0.005;
  return entry === mode;
}
