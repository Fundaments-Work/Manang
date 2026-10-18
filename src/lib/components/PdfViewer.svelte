<script lang="ts" module>
  /** Ids for focused-surface registrations: one per viewer instance. */
  let surfaceCount = 0;
</script>

<script lang="ts">
  import { onDestroy, tick, untrack } from "svelte";
  import type { PDFDocumentLoadingTask, PDFDocumentProxy } from "pdfjs-dist";

  import PdfFindBar from "$lib/components/PdfFindBar.svelte";
  import PdfOutline from "$lib/components/PdfOutline.svelte";
  import { focusedSurface } from "$lib/commands/surface";
  import { reportError } from "$lib/kernel";
  import { openExternal } from "$lib/kernel/native";
  import type { PdfView } from "$lib/state/pdfView.svelte";
  import { calibrateSpans } from "$lib/workbench/pdf/calibrate";
  import {
    findMatches,
    firstFrom,
    hitsByRun,
    matchTop,
    pageText,
    runsOf,
    splitRun,
    stepMatch,
    type FindMatch,
    type PageText,
  } from "$lib/workbench/pdf/find";
  import {
    anchorScroll,
    currentPage,
    layoutPages,
    readingPosition,
    scrollForPosition,
    scrollToPoint,
    type PageLayout,
    type Point,
  } from "$lib/workbench/pdf/layout";
  import {
    flattenOutline,
    linkBox,
    linkTarget,
    resolveDest,
    type Destination,
    type LinkTarget,
    type OutlineNode,
  } from "$lib/workbench/pdf/links";
  import { navigationKey } from "$lib/workbench/pdf/navigation";
  import { documentOptions, pdfjs } from "$lib/workbench/pdf/runtime";
  import {
    registerTextLayer,
    selectedText,
    selectionTouches,
    unregisterTextLayer,
  } from "$lib/workbench/pdf/selection";
  import {
    CSS_PER_POINT,
    MARGINS,
    clampZoom,
    scaleFor,
    wheelFactor,
    zoomKey,
    zoomOf,
    type PageSize,
  } from "$lib/workbench/pdf/zoom";
  import { createBook, freeCanvas, outputRatio } from "$lib/workbench/pdfBook";

  /**
   * PDF viewer (ADR 0006): continuous pages with a text layer (select, copy,
   * find), links, the outline, zoom like Overleaf and source jumps.
   *
   * pdf.js is loaded on first use and parses in its own worker; only the pages
   * near the viewport own a canvas, text layer and links, so memory stays flat
   * on long documents. A new `data` replaces the document without remounting:
   * the scroll position stays and each page keeps its old picture until the
   * new one is drawn, so typing never flashes the preview.
   *
   * Zooming lays the pages out at once (canvases stretch, the text layer
   * follows through `--total-scale-factor`) and redraws them sharply when the
   * zoom settles. Ctrl+wheel and pinch preview with a transform and commit
   * once the gesture pauses (Theorem's approach), anchored at the pointer.
   * All the arithmetic is in `workbench/pdf/`.
   */

  interface Props {
    /** The PDF, raw. */
    data: Uint8Array;
    view: PdfView;
    /** Identifies the document, to come back to the same place later. */
    docKey?: string;
    onerror: (message: string) => void;
    /** Ctrl+click or double-click at `(x, y)` points from the top-left of 0-based `page`. */
    onsourceclick?: (page: number, x: number, y: number) => void;
  }

  const { data, view, docKey = "", onerror, onsourceclick }: Props = $props();

  /** Quiet time after the last Ctrl+wheel event before the zoom is committed. */
  const WHEEL_SETTLE_MS = 180;
  /** Quiet time after a zoom before the visible pages are drawn again, sharp. */
  const REDRAW_MS = 140;

  let root: HTMLDivElement | undefined = $state();
  let scroller: HTMLDivElement | undefined = $state();
  let column: HTMLDivElement | undefined = $state();
  let pane = $state({ width: 0, height: 0 });
  let sizes = $state.raw<readonly PageSize[]>([]);
  let failed = $state("");
  /** Bumped when a new document is shown, for effects that depend on it. */
  let docVersion = $state(0);

  let doc: PDFDocumentProxy | null = null;
  let docTask: PDFDocumentLoadingTask | null = null;
  let generation = 0;
  let views: readonly (readonly number[])[] = [];
  let loadToken = 0;
  const book = createBook();
  const { slots, tasks, drawn, visible, links, highlighted } = book;
  let observer: IntersectionObserver | undefined;
  let frame = 0;
  let redrawTimer: ReturnType<typeof setTimeout> | undefined;

  const widest = $derived(
    sizes.reduce((acc, s) => ({ w: Math.max(acc.w, s.w), h: Math.max(acc.h, s.h) }), {
      w: 0,
      h: 0,
    }),
  );
  const scale = $derived(
    sizes.length === 0 || pane.width === 0
      ? view.scale
      : scaleFor(view.zoom, pane, widest),
  );
  const layout: PageLayout = $derived(layoutPages(sizes, scale, pane.width));

  $effect(() => {
    const s = scale;
    untrack(() => (view.scale = s));
  });

  /* ---------------------------------------------------------------- load */

  async function load(bytes: Uint8Array): Promise<void> {
    const token = ++loadToken;
    try {
      const lib = await pdfjs();
      // pdf.js takes ownership of the buffer it is given, so hand it a copy.
      const loading = lib.getDocument({ data: bytes.slice(), ...documentOptions(lib) });
      const next = await loading.promise;
      if (token !== loadToken) {
        void loading.destroy();
        return;
      }
      const pages = await Promise.all(
        Array.from({ length: next.numPages }, (_, i) => next.getPage(i + 1)),
      );
      if (token !== loadToken) {
        void loading.destroy();
        return;
      }
      const nextSizes = pages.map((p) => {
        const v = p.getViewport({ scale: 1 });
        return { w: v.width, h: v.height };
      });
      const previous = docTask;
      cancelAll();
      doc = next;
      docTask = loading;
      generation += 1;
      views = pages.map((p) => p.view);
      texts = null;
      if (!sameSizes(sizes, nextSizes)) sizes = nextSizes;
      view.pageCount = next.numPages;
      view.pageSizes = sizes;
      failed = "";
      docVersion += 1;
      await tick();
      for (const index of visible) void draw(index);
      void previous?.destroy();
      next
        .getOutline()
        .then((outline) => {
          if (token === loadToken)
            view.outline = flattenOutline(outline as OutlineNode[] | null);
        })
        .catch((error: unknown) => reportError("pdf-viewer.outline", error));
    } catch (error) {
      if (token !== loadToken) return;
      failed = error instanceof Error ? error.message : String(error);
      onerror(`The PDF could not be shown: ${failed}`);
    }
  }

  function sameSizes(a: readonly PageSize[], b: readonly PageSize[]): boolean {
    return (
      a.length === b.length && a.every((s, i) => s.w === b[i]?.w && s.h === b[i]?.h)
    );
  }

  function cancelAll(): void {
    for (const { task, text } of tasks.values()) {
      task.cancel();
      text?.cancel();
    }
    tasks.clear();
  }

  /* ---------------------------------------------------------------- draw */

  async function draw(index: number): Promise<void> {
    const current = doc;
    const gen = generation;
    const slot = slots.get(index);
    if (current === null || slot === undefined || index >= sizes.length) return;
    const wanted = scale;
    const have = drawn.get(index);
    if (have !== undefined && have.generation === gen && have.scale === wanted) return;
    const running = tasks.get(index);
    running?.task.cancel();
    running?.text?.cancel();
    let canvas: HTMLCanvasElement | null = null;
    try {
      const lib = await pdfjs();
      const pdfPage = await current.getPage(index + 1);
      if (gen !== generation) return;
      const viewport = pdfPage.getViewport({ scale: wanted });
      const ratio = outputRatio(
        viewport.width,
        viewport.height,
        window.devicePixelRatio,
      );
      canvas = document.createElement("canvas");
      canvas.width = Math.max(1, Math.floor(viewport.width * ratio));
      canvas.height = Math.max(1, Math.floor(viewport.height * ratio));
      canvas.setAttribute("aria-hidden", "true");
      const context = canvas.getContext("2d");
      if (context === null) return;
      const task = pdfPage.render({
        canvas,
        canvasContext: context,
        viewport,
        transform: ratio === 1 ? undefined : [ratio, 0, 0, ratio, 0, 0],
      });
      const reuse = have !== undefined && have.generation === gen;
      if (reuse) {
        tasks.set(index, { task, page: pdfPage });
        await task.promise;
        if (
          gen !== generation ||
          slots.get(index) !== slot ||
          drawn.get(index) !== have
        ) {
          freeCanvas(canvas);
          return;
        }
        have.canvas.replaceWith(canvas);
        freeCanvas(have.canvas);
        drawn.set(index, { ...have, scale: wanted, canvas });
        return;
      }
      // A new document (or a page drawn for the first time): text and links too,
      // built off-screen and swapped in with the canvas in one step.
      const content = await pdfPage.getTextContent({ includeMarkedContent: true });
      if (gen !== generation) return;
      const textDiv = document.createElement("div");
      textDiv.className = "mng-text-layer";
      textDiv.dataset["textLayer"] = "";
      const textLayer = new lib.TextLayer({
        textContentSource: content,
        container: textDiv,
        viewport,
      });
      tasks.set(index, { task, page: pdfPage, text: textLayer });
      const [, , annotations] = await Promise.all([
        task.promise,
        textLayer.render(),
        pdfPage.getAnnotations({ intent: "display" }),
      ]);
      if (gen !== generation || slots.get(index) !== slot) {
        freeCanvas(canvas);
        return;
      }
      const linkLayer = buildLinks(annotations, pdfPage.view);
      if (have !== undefined) {
        unregisterTextLayer(have.text);
        freeCanvas(have.canvas);
      }
      slot.replaceChildren(canvas, textDiv, linkLayer);
      registerTextLayer(textDiv);
      const runs = textLayer.textDivs;
      drawn.set(index, {
        generation: gen,
        scale: wanted,
        canvas,
        text: textDiv,
        runs,
        strings: textLayer.textContentItemsStr,
      });
      highlighted.delete(index);
      applyFind(index);
      // WebKit lays the runs out with other font metrics than pdf.js measured.
      const widths = content.items.flatMap((item) =>
        "str" in item ? [item.width] : [],
      );
      requestAnimationFrame(() => calibrateSpans(runs, widths, wanted));
    } catch (error) {
      freeCanvas(canvas);
      const name = error instanceof Error ? error.name : "";
      // A cancelled render is routine (scrolled away, zoomed again, new document).
      if (name !== "RenderingCancelledException" && name !== "AbortException") {
        if (gen === generation) reportError("pdf-viewer.draw", error);
      }
    } finally {
      const entry = tasks.get(index);
      if (
        entry !== undefined &&
        entry.page.pageNumber === index + 1 &&
        gen === generation
      ) {
        entry.page.cleanup();
        tasks.delete(index);
      }
    }
  }

  function buildLinks(
    annotations: readonly Record<string, unknown>[],
    pageView: readonly number[],
  ): HTMLElement {
    const layer = document.createElement("div");
    layer.className = "mng-link-layer";
    for (const annotation of annotations) {
      const target = linkTarget(annotation);
      const box = linkBox((annotation["rect"] as number[] | undefined) ?? [], pageView);
      if (target === null || box === null) continue;
      const a = document.createElement("a");
      a.className = "mng-pdf-link";
      a.style.left = `${box.left * 100}%`;
      a.style.top = `${box.top * 100}%`;
      a.style.width = `${box.width * 100}%`;
      a.style.height = `${box.height * 100}%`;
      a.draggable = false;
      a.dataset["pdfLink"] = target.kind;
      if (target.kind === "external") {
        a.href = target.url;
        a.title = target.url;
        a.setAttribute("aria-label", `Open ${target.url} in the browser`);
      } else {
        a.href = "#";
        a.setAttribute("aria-label", "Go to the linked place in the document");
      }
      links.set(a, target);
      layer.append(a);
    }
    return layer;
  }

  function release(index: number): void {
    const running = tasks.get(index);
    running?.task.cancel();
    running?.text?.cancel();
    const d = drawn.get(index);
    // Never pull the text out from under a selection the reader is making.
    if (d !== undefined && selectionTouches(d.text)) return;
    if (d !== undefined) {
      unregisterTextLayer(d.text);
      freeCanvas(d.canvas);
    }
    slots.get(index)?.replaceChildren();
    drawn.delete(index);
    highlighted.delete(index);
  }

  /** Svelte action: a page slot registers itself with the observer. */
  function slot(node: HTMLElement, index: number) {
    slots.set(index, node);
    node.dataset["index"] = String(index);
    observer?.observe(node);
    return {
      destroy() {
        observer?.unobserve(node);
        release(index);
        slots.delete(index);
        visible.delete(index);
      },
    };
  }

  function onIntersect(entries: IntersectionObserverEntry[]): void {
    for (const entry of entries) {
      const index = Number((entry.target as HTMLElement).dataset["index"]);
      if (entry.isIntersecting) {
        visible.add(index);
        void draw(index);
      } else {
        visible.delete(index);
        release(index);
      }
    }
  }

  /* ------------------------------------------------- scrolling and zoom */

  let lastLayout: PageLayout | null = null;
  let lastScroll = { left: 0, top: 0 };
  /** Where the zoom should hold still (pointer of a wheel gesture), once. */
  let pendingAnchor: Point | null = null;

  function track(): void {
    frame = 0;
    const el = scroller;
    if (el === undefined || sizes.length === 0) return;
    lastScroll = { left: el.scrollLeft, top: el.scrollTop };
    view.page = currentPage(layout, el.scrollTop, el.clientHeight);
    view.remember(docKey, readingPosition(layout, el.scrollTop));
  }

  function onscroll(): void {
    if (frame === 0) frame = requestAnimationFrame(track);
  }

  // A new layout: keep the point under the anchor (the pointer of a wheel
  // zoom, else the middle of the pane) where it was when the scale changed;
  // on the first document, go back to where the reader last was.
  $effect(() => {
    const next = layout;
    untrack(() => {
      const el = scroller;
      const before = lastLayout;
      lastLayout = next;
      if (el === undefined || next.tops.length === 0) return;
      if (before === null || before.tops.length === 0) {
        const spot = view.recall(docKey);
        if (spot !== null) el.scrollTop = scrollForPosition(next, spot);
        lastScroll = { left: el.scrollLeft, top: el.scrollTop };
        return;
      }
      if (before.scale === next.scale) return;
      const anchor = pendingAnchor ?? { x: el.clientWidth / 2, y: el.clientHeight / 2 };
      pendingAnchor = null;
      const to = anchorScroll(before, next, anchor, lastScroll);
      el.scrollLeft = to.left;
      el.scrollTop = to.top;
      lastScroll = { left: el.scrollLeft, top: el.scrollTop };
      clearTimeout(redrawTimer);
      redrawTimer = setTimeout(() => {
        for (const index of visible) void draw(index);
      }, REDRAW_MS);
    });
  });

  let gesture: {
    target: number;
    pointer: Point;
    timer: ReturnType<typeof setTimeout>;
  } | null = null;

  function commitWheel(): void {
    const g = gesture;
    if (g === null) return;
    gesture = null;
    clearTimeout(g.timer);
    if (column !== undefined) {
      column.style.transform = "";
      column.style.willChange = "";
    }
    const el = scroller;
    if (el !== undefined) lastScroll = { left: el.scrollLeft, top: el.scrollTop };
    if (Math.abs(g.target * CSS_PER_POINT - scale) < 1e-6) return;
    pendingAnchor = g.pointer;
    view.setZoom(g.target);
  }

  /** Ctrl/Cmd+wheel and pinch zoom here, never the whole webview. */
  function onwheel(event: WheelEvent): void {
    if (!(event.ctrlKey || event.metaKey)) {
      if (gesture !== null) commitWheel();
      return;
    }
    event.preventDefault();
    const el = scroller;
    if (el === undefined || column === undefined) return;
    if (gesture === null) {
      const rect = el.getBoundingClientRect();
      const pointer = { x: event.clientX - rect.left, y: event.clientY - rect.top };
      gesture = { target: zoomOf(scale), pointer, timer: setTimeout(commitWheel, 0) };
      column.style.transformOrigin = `${el.scrollLeft + pointer.x}px ${el.scrollTop + pointer.y}px`;
      column.style.willChange = "transform";
    }
    gesture.target = clampZoom(
      gesture.target * wheelFactor(event.deltaY, event.deltaMode),
    );
    column.style.transform = `scale(${(gesture.target * CSS_PER_POINT) / scale})`;
    clearTimeout(gesture.timer);
    gesture.timer = setTimeout(commitWheel, WHEEL_SETTLE_MS);
  }

  $effect(() => {
    const el = scroller;
    if (el === undefined) return;
    // Not passive: preventDefault keeps the webview from zooming itself.
    el.addEventListener("wheel", onwheel, { passive: false });
    observer = new IntersectionObserver(onIntersect, {
      root: el,
      rootMargin: "100% 0px",
    });
    for (const node of slots.values()) observer.observe(node);
    const resize = new ResizeObserver(() => {
      pane = { width: el.clientWidth, height: el.clientHeight };
    });
    resize.observe(el);
    pane = { width: el.clientWidth, height: el.clientHeight };
    return () => {
      el.removeEventListener("wheel", onwheel);
      observer?.disconnect();
      resize.disconnect();
    };
  });

  $effect(() => {
    const bytes = data;
    untrack(() => void load(bytes));
  });

  // Requests to go somewhere: the page box, outline, links, "Show in preview".
  let lastJump = untrack(() => view.jump);
  $effect(() => {
    const jump = view.jump;
    void docVersion;
    untrack(() => {
      const el = scroller;
      if (el === undefined || layout.tops.length === 0 || jump === lastJump) return;
      lastJump = jump;
      const page = view.page;
      const mark = view.mark;
      const y = view.jumpY;
      let top: number;
      if (mark !== null && mark.page === page)
        top = scrollToPoint(layout, page, mark.y, el.clientHeight);
      else if (y !== null)
        top = Math.max(0, (layout.tops[page] ?? 0) + y * layout.scale - MARGINS.gap);
      else top = scrollToPoint(layout, page, null, el.clientHeight);
      el.scrollTo({ top });
    });
  });

  /** Where a mark sits inside the column of pages, in CSS px. */
  const markBox = $derived.by(() => {
    const mark = view.mark;
    if (mark === null || mark.page >= sizes.length) return null;
    return {
      left: (layout.lefts[mark.page] ?? 0) + mark.x * scale - 2,
      top: (layout.tops[mark.page] ?? 0) + mark.y * scale - 2,
      width: Math.max(8, mark.width * scale + 4),
      height: Math.max(8, mark.height * scale + 4),
    };
  });

  /* ---------------------------------------------------------------- find */

  let texts: Promise<PageText[]> | null = null;
  let matches: FindMatch[] = [];

  function pageTexts(): Promise<PageText[]> {
    const current = doc;
    if (current === null) return Promise.resolve([]);
    texts ??= Promise.all(
      Array.from({ length: current.numPages }, async (_, i) => {
        const page = await current.getPage(i + 1);
        const content = await page.getTextContent();
        return pageText(runsOf(content.items, views[i] ?? page.view));
      }),
    );
    return texts;
  }

  let pageTextCache: PageText[] = [];

  function applyFind(index: number): void {
    const d = drawn.get(index);
    if (d === undefined) return;
    for (const run of highlighted.get(index) ?? []) {
      const el = d.runs[run];
      if (el !== undefined) el.textContent = d.strings[run] ?? "";
    }
    highlighted.delete(index);
    const page = pageTextCache[index];
    if (!view.findOpen || page === undefined) return;
    const hits = hitsByRun(page, matches, index, view.findCurrent);
    for (const [run, list] of hits) {
      const el = d.runs[run];
      const text = d.strings[run];
      if (el === undefined || text === undefined) continue;
      el.replaceChildren(
        ...splitRun(text, list).map((piece) => {
          if (piece.hit === "none") return document.createTextNode(piece.text);
          const span = document.createElement("span");
          span.className = "mng-hit";
          if (piece.hit === "current") span.dataset["current"] = "";
          span.textContent = piece.text;
          return span;
        }),
      );
    }
    if (hits.size > 0) highlighted.set(index, new Set(hits.keys()));
  }

  function applyFindAll(): void {
    for (const index of new Set([...drawn.keys(), ...highlighted.keys()]))
      applyFind(index);
  }

  function scrollToMatch(): void {
    const el = scroller;
    const match = matches[view.findCurrent];
    const page = match === undefined ? undefined : pageTextCache[match.page];
    if (el === undefined || match === undefined || page === undefined) return;
    el.scrollTo({
      top: scrollToPoint(layout, match.page, matchTop(page, match), el.clientHeight),
    });
  }

  let findRun = 0;
  let lastQuery = "";
  $effect(() => {
    const open = view.findOpen;
    const query = view.findQuery;
    const caseSensitive = view.findCase;
    void docVersion;
    untrack(() => {
      const run = ++findRun;
      if (!open || query.trim() === "") {
        matches = [];
        view.findTotal = 0;
        view.findCurrent = -1;
        lastQuery = "";
        applyFindAll();
        return;
      }
      void pageTexts()
        .then((all) => {
          if (run !== findRun) return;
          pageTextCache = all;
          matches = findMatches(all, query, { caseSensitive });
          const changed = `${query}\u0000${caseSensitive}` !== lastQuery;
          lastQuery = `${query}\u0000${caseSensitive}`;
          view.findTotal = matches.length;
          view.findCurrent = changed
            ? firstFrom(matches, view.page)
            : Math.min(view.findCurrent, matches.length - 1);
          applyFindAll();
          if (changed) scrollToMatch();
        })
        .catch((error: unknown) => reportError("pdf-viewer.find", error));
    });
  });

  let lastStep = untrack(() => view.findStep.n);
  $effect(() => {
    const step = view.findStep;
    untrack(() => {
      if (step.n === lastStep) return;
      lastStep = step.n;
      if (matches.length === 0) return;
      view.findCurrent = stepMatch(view.findCurrent, matches.length, step.direction);
      applyFindAll();
      scrollToMatch();
    });
  });

  /* ------------------------------------------------ pointer and keyboard */

  /** The page point under a pointer event, in points from the page's top-left. */
  function pagePoint(event: MouseEvent): { page: number; x: number; y: number } | null {
    const slotEl = (event.target as Element | null)?.closest<HTMLElement>(
      "[data-index]",
    );
    if (slotEl === null || slotEl === undefined) return null;
    const index = Number(slotEl.dataset["index"]);
    const size = sizes[index];
    const rect = slotEl.getBoundingClientRect();
    if (!Number.isInteger(index) || size === undefined || rect.width === 0) return null;
    const ratio = size.w / rect.width;
    return {
      page: index,
      x: (event.clientX - rect.left) * ratio,
      y: (event.clientY - rect.top) * ratio,
    };
  }

  function toSource(event: MouseEvent): void {
    const point = pagePoint(event);
    if (point === null || onsourceclick === undefined) return;
    onsourceclick(point.page, point.x, point.y);
  }

  async function follow(target: LinkTarget): Promise<void> {
    try {
      if (target.kind === "external") {
        await openExternal(target.url);
        return;
      }
      const to = await goTo(target.dest);
      if (to === null) onerror("That link leads nowhere in this document.");
    } catch (error) {
      reportError("pdf-viewer.link", error);
    }
  }

  async function goTo(dest: Destination): Promise<{ page: number } | null> {
    const current = doc;
    if (current === null) return null;
    const to = await resolveDest(dest, {
      getDestination: (name) => current.getDestination(name),
      getPageIndex: (ref) => current.getPageIndex(ref),
      view: (page) => views[page] ?? null,
    });
    if (to === null || current !== doc) return null;
    view.go(to.page, to.y);
    return to;
  }

  function onmousedown(event: MouseEvent): void {
    // Pages are not focusable themselves; keep the keys (zoom, find) here.
    if (!(root?.contains(document.activeElement) ?? false))
      scroller?.focus({ preventScroll: true });
    if (event.button !== 0) return;
    if (event.ctrlKey || event.metaKey) {
      // Ctrl+click goes to the source; it must not start a selection.
      event.preventDefault();
      scroller?.focus({ preventScroll: true });
    } else if (
      event.detail === 2 &&
      view.doubleClickSource &&
      !event.altKey &&
      onsourceclick !== undefined
    ) {
      // Double-click goes to the source, so the browser must not select a word.
      event.preventDefault();
    }
  }

  function onclick(event: MouseEvent): void {
    const link = (event.target as Element | null)?.closest("a[data-pdf-link]");
    if (event.ctrlKey || event.metaKey) {
      // Ctrl+click on a link too goes to the source, not to the link.
      event.preventDefault();
      if (link !== null && link !== undefined) event.stopPropagation();
      toSource(event);
      return;
    }
    if (link === null || link === undefined) return;
    // Never navigate the webview; the shell's own link router must not open
    // it a second time.
    event.preventDefault();
    event.stopPropagation();
    const target = links.get(link);
    if (target !== undefined) void follow(target);
  }

  function onauxclick(event: MouseEvent): void {
    if ((event.target as Element | null)?.closest("a[data-pdf-link]"))
      event.preventDefault();
  }

  function ondblclick(event: MouseEvent): void {
    if (!view.doubleClickSource || event.altKey || onsourceclick === undefined) return;
    if ((event.target as Element | null)?.closest("a[data-pdf-link]")) return;
    window.getSelection()?.removeAllRanges();
    toSource(event);
  }

  /** Copy the selected PDF text as plain text, whitespace tidied. */
  function oncopy(event: ClipboardEvent): void {
    const selection = document.getSelection();
    const anchor = selection?.anchorNode ?? null;
    if (anchor === null || !(root?.contains(anchor) ?? false)) return;
    const text = selectedText(selection);
    if (text === "" || event.clipboardData === null) return;
    event.preventDefault();
    event.clipboardData.setData("text/plain", text);
  }

  function isField(target: EventTarget | null): boolean {
    return (
      target instanceof Element &&
      target.closest("input, textarea, select, button, [contenteditable='true']") !==
        null
    );
  }

  function onkeydown(event: KeyboardEvent): void {
    const zoom = zoomKey(event);
    if (zoom !== null) {
      if (zoom === "in") view.zoomIn();
      else if (zoom === "out") view.zoomOut();
      else view.fit();
    } else if ((event.ctrlKey || event.metaKey) && !event.altKey && event.key === "f") {
      view.openFind();
    } else if (event.key === "Escape" && view.findOpen) {
      view.closeFind();
      scroller?.focus({ preventScroll: true });
    } else {
      if (isField(event.target)) return;
      const action = navigationKey(event);
      const el = scroller;
      if (action === null || el === undefined) return;
      if (action.kind === "page") view.go(view.page + action.delta);
      else if (action.kind === "first") view.go(0);
      else if (action.kind === "last") view.go(sizes.length - 1);
      else if (action.kind === "scroll") el.scrollBy(action.dx, action.dy);
      else el.scrollBy(0, action.direction * el.clientHeight * 0.9);
    }
    event.preventDefault();
    event.stopPropagation();
  }

  // Edit > Copy, Select all and Find go to this viewer while it has the focus.
  // One id per instance: a preview and a PDF tab can both be open.
  const stopSurface = focusedSurface.register({
    id: `pdf-${++surfaceCount}`,
    contains: (element) => root?.contains(element as Node) ?? false,
    available: () => root !== undefined,
    actions: {
      copy: async () => {
        const text = selectedText(document.getSelection());
        if (text === "") return;
        try {
          await navigator.clipboard.writeText(text);
        } catch (error) {
          reportError("the selected PDF text could not be copied", error);
        }
      },
      selectAll: () => {
        if (column !== undefined) document.getSelection()?.selectAllChildren(column);
      },
      find: () => view.openFind(),
    },
  });

  onDestroy(() => {
    stopSurface();
    loadToken += 1;
    findRun += 1;
    cancelAll();
    if (frame !== 0) cancelAnimationFrame(frame);
    clearTimeout(redrawTimer);
    if (gesture !== null) clearTimeout(gesture.timer);
    for (const d of drawn.values()) {
      unregisterTextLayer(d.text);
      freeCanvas(d.canvas);
    }
    drawn.clear();
    void docTask?.destroy();
    doc = null;
    docTask = null;
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  bind:this={root}
  class="absolute inset-0 flex"
  data-dark-pages={view.darkPages ? "" : undefined}
  {onkeydown}
  {oncopy}
>
  {#if view.outlineOpen && view.outline.length > 0}
    <PdfOutline
      entries={view.outline}
      onselect={(entry) => {
        if (entry.dest !== null)
          void goTo(entry.dest).catch((error: unknown) =>
            reportError("pdf-viewer.outline", error),
          );
        else if (entry.url !== null) void follow({ kind: "external", url: entry.url });
      }}
    />
  {/if}
  <div class="relative min-w-0 flex-1">
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
    <div
      bind:this={scroller}
      class="mng-pdf-scroller absolute inset-0 overflow-auto outline-none"
      data-pdf-viewer
      data-pdf-scroller
      tabindex="0"
      role="document"
      aria-label="PDF pages. Ctrl+F finds text, Ctrl+plus and Ctrl+minus zoom."
      {onscroll}
      {onmousedown}
      {onclick}
      {onauxclick}
      {ondblclick}
    >
      <div
        bind:this={column}
        class="relative"
        style="width: {layout.contentWidth}px; height: {layout.contentHeight}px"
      >
        {#each sizes as _, index (index)}
          <div
            use:slot={index}
            class="mng-pdf-page absolute overflow-hidden bg-paper outline outline-line"
            style="left: {layout.lefts[index]}px; top: {layout.tops[
              index
            ]}px; width: {layout.widths[index]}px; height: {layout.heights[
              index
            ]}px; --total-scale-factor: {scale}"
            data-page={index + 1}
            role="group"
            aria-label="Page {index + 1} of {sizes.length}"
          ></div>
        {/each}
        {#if markBox !== null}
          {#key view.mark?.id}
            <div
              class="mng-preview-mark pointer-events-none absolute z-10"
              style="left: {markBox.left}px; top: {markBox.top}px; width: {markBox.width}px; height: {markBox.height}px"
              data-preview-mark
              aria-hidden="true"
            ></div>
          {/key}
        {/if}
      </div>
      {#if failed !== ""}
        <p class="m-0 px-6 pt-6 text-xs text-text-60" role="alert">{failed}</p>
      {/if}
    </div>
    {#if view.findOpen}
      <PdfFindBar {view} onclose={() => scroller?.focus({ preventScroll: true })} />
    {/if}
  </div>
</div>

<style>
  .mng-pdf-scroller {
    /* The zoom anchors the scroll position itself. */
    overflow-anchor: none;
    scrollbar-gutter: stable;
  }
  .mng-pdf-page {
    --scale-round-x: 1px;
    --scale-round-y: 1px;
  }
  .mng-pdf-page :global(canvas) {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    z-index: 1;
  }
  /* Dark pages: invert the picture only, keeping hues; the text layer and its
     selection are untouched. */
  [data-dark-pages] .mng-pdf-page {
    background: var(--mng-paper-ink);
  }
  [data-dark-pages] .mng-pdf-page :global(canvas) {
    filter: invert(1) hue-rotate(180deg);
  }

  /* pdf.js text layer: transparent runs over the canvas, for selection, copy,
     find and screen readers. After pdf.js' viewer CSS and Theorem's. */
  .mng-pdf-page :global(.mng-text-layer) {
    position: absolute;
    inset: 0;
    z-index: 2;
    overflow: clip;
    opacity: 1;
    line-height: 1;
    letter-spacing: normal;
    word-spacing: normal;
    text-align: initial;
    text-size-adjust: none;
    -webkit-text-size-adjust: none;
    forced-color-adjust: none;
    transform-origin: 0 0;
    caret-color: CanvasText;
    user-select: text;
    -webkit-user-select: text;
    --min-font-size: 1;
    --text-scale-factor: calc(var(--total-scale-factor) * var(--min-font-size));
    --min-font-size-inv: calc(1 / var(--min-font-size));
  }
  .mng-pdf-page :global(.mng-text-layer :is(span, br)) {
    position: absolute;
    color: transparent;
    -webkit-text-fill-color: transparent;
    white-space: pre;
    cursor: text;
    transform-origin: 0% 0%;
    user-select: text;
    -webkit-user-select: text;
  }
  .mng-pdf-page :global(.mng-text-layer > :not(.markedContent)),
  .mng-pdf-page :global(.mng-text-layer .markedContent span:not(.markedContent)) {
    z-index: 1;
    --font-height: 0;
    font-size: calc(var(--text-scale-factor) * var(--font-height));
    --scale-x: 1;
    --rotate: 0deg;
    transform: rotate(var(--rotate)) scaleX(var(--scale-x))
      scale(var(--min-font-size-inv));
  }
  .mng-pdf-page :global(.mng-text-layer .markedContent) {
    display: contents;
  }
  .mng-pdf-page :global(.mng-text-layer span[role="img"]) {
    user-select: none;
    -webkit-user-select: none;
    cursor: default;
  }
  .mng-pdf-page :global(.mng-text-layer ::selection) {
    background: color-mix(in srgb, var(--mng-accent) 40%, transparent);
    color: transparent;
    -webkit-text-fill-color: transparent;
  }
  .mng-pdf-page :global(.mng-text-layer br::selection) {
    background: transparent;
  }
  .mng-pdf-page :global(.mng-text-layer .endOfContent) {
    display: block;
    position: absolute;
    inset: 100% 0 0;
    z-index: 0;
    cursor: default;
    user-select: none;
    -webkit-user-select: none;
  }
  .mng-pdf-page :global(.mng-text-layer.selecting .endOfContent) {
    top: 0;
  }
  /* Find: other matches a quiet ink tone, the current one the accent. */
  .mng-pdf-page :global(.mng-text-layer span.mng-hit) {
    position: static !important;
    display: inline;
    font-size: inherit !important;
    transform: none !important;
    background: color-mix(in srgb, var(--mng-paper-ink-60) 30%, transparent);
  }
  .mng-pdf-page :global(.mng-text-layer span.mng-hit[data-current]) {
    background: color-mix(in srgb, var(--mng-accent) 45%, transparent);
    outline: var(--hairline, 1px) solid var(--mng-accent);
  }

  /* Links: above the text so they are clickable; only the boxes take the
     pointer, so selection works everywhere else. */
  .mng-pdf-page :global(.mng-link-layer) {
    position: absolute;
    inset: 0;
    z-index: 3;
    pointer-events: none;
  }
  .mng-pdf-page :global(.mng-pdf-link) {
    position: absolute;
    pointer-events: auto;
    cursor: pointer;
    -webkit-user-drag: none;
  }
  .mng-pdf-page :global(.mng-pdf-link:hover) {
    background: color-mix(in srgb, var(--mng-paper-ink-60) 16%, transparent);
  }
</style>
