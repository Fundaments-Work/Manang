/**
 * Bookkeeping for the PDF viewer: which page slot is which element, which pages
 * are near the viewport, what is being drawn and at what scale. Plain
 * (non-reactive) collections on purpose: the viewer updates the DOM itself and
 * must not re-render when they change.
 */

import type { RenderTask, PDFPageProxy } from "pdfjs-dist";

export interface PageBook {
  readonly slots: Map<number, HTMLElement>;
  readonly tasks: Map<number, { task: RenderTask; page: PDFPageProxy }>;
  /** Scale each page's canvas was drawn at. */
  readonly drawn: Map<number, number>;
  readonly visible: Set<number>;
}

export function createBook(): PageBook {
  return { slots: new Map(), tasks: new Map(), drawn: new Map(), visible: new Set() };
}
