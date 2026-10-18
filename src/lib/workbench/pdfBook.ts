/**
 * Bookkeeping for the PDF viewer: which page slot is which element, which pages
 * are near the viewport, what is being drawn, and what each drawn page holds
 * (its canvas, text layer and links, for which document and at what scale).
 * Plain (non-reactive) collections on purpose: the viewer updates the DOM
 * itself and must not re-render when they change.
 */

import type { PDFPageProxy, RenderTask, TextLayer } from "pdfjs-dist";

import type { LinkTarget } from "./pdf/links";

export interface DrawnPage {
  /** Document generation the layers belong to. */
  readonly generation: number;
  /** Scale (CSS px per point) the canvas was drawn at. */
  readonly scale: number;
  readonly canvas: HTMLCanvasElement;
  readonly text: HTMLElement;
  /** The text layer's runs and their strings, aligned with the page's text items. */
  readonly runs: readonly HTMLElement[];
  readonly strings: readonly string[];
}

export interface PageBook {
  readonly slots: Map<number, HTMLElement>;
  readonly tasks: Map<
    number,
    { task: RenderTask; page: PDFPageProxy; text?: TextLayer }
  >;
  readonly drawn: Map<number, DrawnPage>;
  readonly visible: Set<number>;
  /** Where each link element leads. */
  readonly links: WeakMap<Element, LinkTarget>;
  /** Text-layer runs that carry find highlights, per page, to undo them. */
  readonly highlighted: Map<number, Set<number>>;
}

export function createBook(): PageBook {
  return {
    slots: new Map(),
    tasks: new Map(),
    drawn: new Map(),
    visible: new Set(),
    links: new WeakMap(),
    highlighted: new Map(),
  };
}

/** Most pixels one page canvas may have (64 MB of RGBA); beyond, it is drawn softer. */
export const MAX_CANVAS_PIXELS = 2 ** 24;

/**
 * Device pixels per CSS px for a page canvas of `width` x `height` CSS px:
 * the screen's ratio (sharp on HiDPI at every zoom), lowered only when the
 * canvas would exceed `MAX_CANVAS_PIXELS`.
 */
export function outputRatio(
  width: number,
  height: number,
  devicePixelRatio: number,
  maxPixels: number = MAX_CANVAS_PIXELS,
): number {
  const dpr =
    Number.isFinite(devicePixelRatio) && devicePixelRatio > 0 ? devicePixelRatio : 1;
  const area = Math.max(1, width * height);
  return Math.min(dpr, Math.sqrt(maxPixels / area));
}

/** Give a canvas' memory back now rather than at the next collection. */
export function freeCanvas(canvas: HTMLCanvasElement | null | undefined): void {
  if (canvas === null || canvas === undefined) return;
  canvas.width = 0;
  canvas.height = 0;
}
