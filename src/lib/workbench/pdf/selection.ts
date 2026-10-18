/**
 * Text selection over pdf.js text layers (DOM side).
 *
 * Text-layer runs are absolutely positioned spans with gaps between them; a
 * drag that crosses a gap would otherwise hit the page behind the layer and
 * the selection would jump to the end of the page. pdf.js' viewer solves this
 * with an `.endOfContent` block that is moved right after the run under the
 * selection's moving end and, while selecting, covers the layer. This module
 * does the same for every layer the viewer registers, with one set of
 * document listeners for all of them.
 *
 * Adapted from Theorem (`pdfjs-engine.tsx`, `ensureGlobalTextLayerSelection-
 * Listeners`, MIT, same owner), which follows pdf.js' `TextLayerBuilder`.
 * Edge auto-scroll while dragging is Theorem's too.
 */

export const SELECTING = "selecting";

/** Registered layers and their end-of-content block. */
const layers = new Map<HTMLElement, HTMLElement>();
let controller: AbortController | null = null;

function reset(end: HTMLElement, layer: HTMLElement): void {
  layer.append(end);
  end.style.width = "";
  end.style.height = "";
  layer.classList.remove(SELECTING);
}

function resetAll(): void {
  layers.forEach((end, layer) => reset(end, layer));
}

function scrollParent(node: HTMLElement): HTMLElement | null {
  return node.closest<HTMLElement>("[data-pdf-scroller]");
}

function listen(): void {
  if (controller !== null) return;
  controller = new AbortController();
  const { signal } = controller;
  let pointerDown = false;
  let previous: Range | null = null;
  let frame = 0;
  let velocity = 0;
  let target: HTMLElement | null = null;

  const stopScroll = (): void => {
    if (frame !== 0) cancelAnimationFrame(frame);
    frame = 0;
    velocity = 0;
    target = null;
  };
  const step = (): void => {
    if (!pointerDown || target === null || velocity === 0) {
      stopScroll();
      return;
    }
    target.scrollTop += velocity;
    frame = requestAnimationFrame(step);
  };

  document.addEventListener("pointerdown", () => (pointerDown = true), { signal });
  document.addEventListener(
    "pointerup",
    () => {
      pointerDown = false;
      stopScroll();
      resetAll();
    },
    { signal },
  );
  document.addEventListener(
    "pointercancel",
    () => {
      pointerDown = false;
      stopScroll();
    },
    { signal },
  );
  window.addEventListener(
    "blur",
    () => {
      pointerDown = false;
      stopScroll();
      resetAll();
    },
    { signal },
  );
  document.addEventListener(
    "keyup",
    () => {
      if (!pointerDown) resetAll();
    },
    { signal },
  );

  // Dragging a selection to the top or bottom edge scrolls the pages.
  document.addEventListener(
    "pointermove",
    (event) => {
      if (!pointerDown) return;
      let layer: HTMLElement | null = null;
      for (const candidate of layers.keys()) {
        if (candidate.classList.contains(SELECTING)) {
          layer = candidate;
          break;
        }
      }
      const scroller = layer === null ? null : scrollParent(layer);
      if (scroller === null) {
        stopScroll();
        return;
      }
      const rect = scroller.getBoundingClientRect();
      const edge = 48;
      let v = 0;
      if (event.clientY < rect.top + edge)
        v = -Math.max(
          2,
          Math.round(24 * Math.min(1, (rect.top + edge - event.clientY) / edge)),
        );
      else if (event.clientY > rect.bottom - edge)
        v = Math.max(
          2,
          Math.round(24 * Math.min(1, (event.clientY - (rect.bottom - edge)) / edge)),
        );
      if (v === 0) {
        stopScroll();
        return;
      }
      target = scroller;
      velocity = v;
      if (frame === 0) frame = requestAnimationFrame(step);
    },
    { signal, passive: true },
  );

  // Synchronous on purpose (as in pdf.js): deferring paints a frame with the
  // selection snapped to the end of the page, which reads as flicker.
  document.addEventListener(
    "selectionchange",
    () => {
      const selection = document.getSelection();
      if (selection === null || selection.rangeCount === 0) {
        if (!pointerDown) resetAll();
        return;
      }
      const touched = new Set<HTMLElement>();
      for (let i = 0; i < selection.rangeCount; i += 1) {
        const range = selection.getRangeAt(i);
        for (const layer of layers.keys()) {
          try {
            if (range.intersectsNode(layer)) touched.add(layer);
          } catch {
            // A range in a detached tree cannot intersect a live layer.
          }
        }
      }
      if (touched.size === 0) {
        if (!pointerDown) resetAll();
        return;
      }
      for (const [layer, end] of layers) {
        if (touched.has(layer)) layer.classList.add(SELECTING);
        else reset(end, layer);
      }
      const range = selection.getRangeAt(0);
      const startMoved =
        previous !== null &&
        (range.compareBoundaryPoints(Range.END_TO_END, previous) === 0 ||
          range.compareBoundaryPoints(Range.START_TO_END, previous) === 0);
      let anchor: Node | null = startMoved ? range.startContainer : range.endContainer;
      if (anchor?.nodeType === Node.TEXT_NODE) anchor = anchor.parentNode;
      const element = anchor instanceof HTMLElement ? anchor : null;
      const layer = element?.closest<HTMLElement>("[data-text-layer]") ?? null;
      const end = layer === null ? undefined : layers.get(layer);
      if (layer !== null && end !== undefined && element !== null) {
        end.style.width = layer.style.width;
        end.style.height = layer.style.height;
        element.parentElement?.insertBefore(
          end,
          startMoved ? element : element.nextSibling,
        );
      }
      previous = range.cloneRange();
    },
    { signal },
  );
  signal.addEventListener("abort", stopScroll, { once: true });
}

/** Track a text layer; returns its end-of-content block (already appended). */
export function registerTextLayer(layer: HTMLElement): HTMLElement {
  const end = document.createElement("div");
  end.className = "endOfContent";
  layer.append(end);
  layers.set(layer, end);
  layer.addEventListener("pointerdown", () => layer.classList.add(SELECTING));
  listen();
  return end;
}

export function unregisterTextLayer(layer: HTMLElement): void {
  layers.delete(layer);
  if (layers.size > 0) return;
  controller?.abort();
  controller = null;
}

/** True when the document selection touches `node` (keep that layer alive). */
export function selectionTouches(node: Node): boolean {
  const selection = document.getSelection();
  if (selection === null || selection.rangeCount === 0 || selection.isCollapsed)
    return false;
  for (let i = 0; i < selection.rangeCount; i += 1) {
    try {
      if (selection.getRangeAt(i).intersectsNode(node)) return true;
    } catch {
      // Detached ranges do not intersect.
    }
  }
  return false;
}

/**
 * The selected PDF text for the clipboard: runs carry their own spacing, so
 * runs of whitespace collapse, and line ends become spaces or newlines as the
 * browser reports them.
 */
export function selectedText(selection: Selection | null): string {
  if (selection === null || selection.isCollapsed) return "";
  return selection
    .toString()
    .replace(/[ \t\u00a0]+/g, " ")
    .replace(/ ?\n ?/g, "\n");
}
