<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import type { PDFDocumentLoadingTask, PDFDocumentProxy } from "pdfjs-dist";

  import type { PreviewMark } from "$lib/state/preview.svelte";

  import { createBook } from "$lib/workbench/pdfBook";

  /**
   * PDF viewer (ADR 0006). pdf.js is loaded on first use, parses in its own
   * worker thread from the bundled worker file (no network, no eval) and only
   * the pages near the viewport own a canvas; a page that scrolls away gives its
   * canvas back, so memory stays flat on long documents.
   *
   * A new `data` replaces the document without remounting: the scroll container
   * and every page slot keep their size, so the reader stays where they were
   * while the new pages draw over the old ones.
   */

  interface Props {
    /** The PDF, raw. */
    data: Uint8Array;
    /** CSS zoom (1 is 640 px wide); `null` fits the width. */
    zoom: number | null;
    /** Changes whenever the user asked to go to `page`. */
    jump: number;
    /** 0-based page the user asked for. */
    page: number;
    /** A box (points, top-left origin) to outline and scroll to, or null. */
    mark?: PreviewMark | null;
    onpage: (page: number) => void;
    onpages: (count: number) => void;
    onerror: (message: string) => void;
    /** Ctrl+click (Cmd+click) at `(x, y)` points from the top-left of 0-based `page`. */
    onsourceclick?: (page: number, x: number, y: number) => void;
  }

  const {
    data,
    zoom,
    jump,
    page,
    mark = null,
    onpage,
    onpages,
    onerror,
    onsourceclick,
  }: Props = $props();

  const GAP = 16;
  const SIDE = 24;
  const MAX_FIT = 920;
  const BASE_WIDTH = 640;
  /** Retina sharpness without quadrupling memory on large pages. */
  const MAX_DPR = 2;

  let scroller: HTMLDivElement | undefined = $state();
  let paneWidth = $state(0);
  let count = $state(0);
  /** Size of one page at scale 1, in CSS px (pages are assumed alike until rendered). */
  let base = $state({ w: 595, h: 842 });
  let failed = $state("");

  let doc: PDFDocumentProxy | null = null;
  let docTask: PDFDocumentLoadingTask | null = null;
  let loadToken = 0;
  const book = createBook();
  const { slots, tasks, drawn, visible } = book;
  let observer: IntersectionObserver | undefined;
  let frame = 0;

  const cssWidth = $derived(
    zoom === null
      ? Math.max(200, Math.min(MAX_FIT, paneWidth - SIDE * 2))
      : Math.round(BASE_WIDTH * zoom),
  );
  const scale = $derived(cssWidth / base.w);
  const slotHeight = $derived(Math.round(base.h * scale));

  async function load(bytes: Uint8Array): Promise<void> {
    const token = ++loadToken;
    try {
      const pdfjs = await import("pdfjs-dist");
      const worker = (await import("pdfjs-dist/build/pdf.worker.min.mjs?url")).default;
      pdfjs.GlobalWorkerOptions.workerSrc = worker;
      // pdf.js takes ownership of the buffer it is given, so hand it a copy.
      const loading = pdfjs.getDocument({
        data: bytes.slice(),
        enableXfa: false,
        useWorkerFetch: false,
      });
      const next = await loading.promise;
      if (token !== loadToken) {
        void loading.destroy();
        return;
      }
      const first = await next.getPage(1);
      const view = first.getViewport({ scale: 1 });
      const previous = docTask;
      doc = next;
      docTask = loading;
      base = { w: view.width, h: view.height };
      first.cleanup();
      count = next.numPages;
      onpages(count);
      failed = "";
      // Drop what belonged to the old document and draw what is on screen.
      cancelAll();
      drawn.clear();
      void previous?.destroy();
      for (const index of visible) void draw(index);
    } catch (error) {
      if (token !== loadToken) return;
      failed = error instanceof Error ? error.message : String(error);
      onerror(`The PDF could not be shown: ${failed}`);
    }
  }

  function cancelAll(): void {
    for (const { task } of tasks.values()) task.cancel();
    tasks.clear();
  }

  async function draw(index: number): Promise<void> {
    const current = doc;
    const slot = slots.get(index);
    if (current === null || slot === undefined || index >= count) return;
    const wanted = scale;
    if (drawn.get(index) === wanted && slot.firstElementChild !== null) return;
    tasks.get(index)?.task.cancel();
    try {
      const pdfPage = await current.getPage(index + 1);
      if (current !== doc) return;
      const viewport = pdfPage.getViewport({ scale: wanted });
      const ratio = Math.min(window.devicePixelRatio || 1, MAX_DPR);
      const canvas = document.createElement("canvas");
      canvas.width = Math.floor(viewport.width * ratio);
      canvas.height = Math.floor(viewport.height * ratio);
      canvas.style.width = `${Math.floor(viewport.width)}px`;
      canvas.style.height = `${Math.floor(viewport.height)}px`;
      const context = canvas.getContext("2d");
      if (context === null) return;
      const task = pdfPage.render({
        canvas,
        canvasContext: context,
        viewport,
        transform: ratio === 1 ? undefined : [ratio, 0, 0, ratio, 0, 0],
      });
      tasks.set(index, { task, page: pdfPage });
      await task.promise;
      if (current !== doc || !slots.has(index)) return;
      slot.replaceChildren(canvas);
      drawn.set(index, wanted);
    } catch (error) {
      // A cancelled render is routine (scrolled away, zoomed again).
      if (!(error instanceof Error) || error.name !== "RenderingCancelledException") {
        if (current === doc) failed = error instanceof Error ? error.message : "";
      }
    } finally {
      const entry = tasks.get(index);
      if (entry !== undefined) {
        entry.page.cleanup();
        tasks.delete(index);
      }
    }
  }

  function release(index: number): void {
    tasks.get(index)?.task.cancel();
    slots.get(index)?.replaceChildren();
    drawn.delete(index);
  }

  /** Svelte action: a page slot registers itself with the observer. */
  function slot(node: HTMLElement, index: number) {
    slots.set(index, node);
    node.dataset["index"] = String(index);
    observer?.observe(node);
    return {
      destroy() {
        observer?.unobserve(node);
        slots.delete(index);
        visible.delete(index);
        tasks.get(index)?.task.cancel();
        drawn.delete(index);
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

  /** Which page the reader is on: the one covering the top third of the pane. */
  function track(): void {
    frame = 0;
    const el = scroller;
    if (el === undefined || count === 0) return;
    const probe = el.scrollTop + el.clientHeight / 3;
    const step = slotHeight + GAP;
    const index = Math.max(0, Math.min(count - 1, Math.floor((probe - GAP) / step)));
    onpage(index);
  }

  function onscroll(): void {
    if (frame === 0) frame = requestAnimationFrame(track);
  }

  $effect(() => {
    const el = scroller;
    if (el === undefined) return;
    observer = new IntersectionObserver(onIntersect, {
      root: el,
      rootMargin: "100% 0px",
    });
    for (const node of slots.values()) observer.observe(node);
    const resize = new ResizeObserver(() => {
      paneWidth = el.clientWidth;
    });
    resize.observe(el);
    paneWidth = el.clientWidth;
    return () => {
      observer?.disconnect();
      resize.disconnect();
    };
  });

  $effect(() => {
    const bytes = data;
    untrack(() => void load(bytes));
  });

  // A new zoom changes every slot's size; redraw the visible pages crisply.
  $effect(() => {
    void scale;
    untrack(() => {
      for (const index of visible) void draw(index);
    });
  });

  $effect(() => {
    void jump;
    untrack(() => {
      const el = scroller;
      if (el === undefined) return;
      const target = slots.get(page);
      const top =
        target === undefined ? page * (slotHeight + GAP) + GAP : target.offsetTop;
      // A mark on this page: bring it to the upper third of the pane.
      const within =
        mark !== null && mark.page === page
          ? Math.max(0, mark.y * scale - el.clientHeight / 3)
          : -GAP;
      el.scrollTo({ top: top + within });
    });
  });

  /** Where a mark sits inside the column of pages, in CSS px. */
  const markBox = $derived.by(() => {
    if (mark === null || mark.page >= count) return null;
    return {
      left: SIDE + mark.x * scale - 2,
      top: GAP + mark.page * (slotHeight + GAP) + mark.y * scale - 2,
      width: Math.max(8, mark.width * scale + 4),
      height: Math.max(8, mark.height * scale + 4),
    };
  });

  /** Ctrl+click (Cmd+click) on a page asks for the source behind that point. */
  function onclick(event: MouseEvent): void {
    if (!(event.ctrlKey || event.metaKey) || onsourceclick === undefined) return;
    const slotEl = (event.target as HTMLElement | null)?.closest<HTMLElement>(
      "[data-index]",
    );
    if (slotEl === null || slotEl === undefined) return;
    const index = Number(slotEl.dataset["index"]);
    const rect = slotEl.getBoundingClientRect();
    if (!Number.isInteger(index) || rect.width === 0) return;
    event.preventDefault();
    const ratio = base.w / rect.width;
    onsourceclick(
      index,
      (event.clientX - rect.left) * ratio,
      (event.clientY - rect.top) * ratio,
    );
  }

  onDestroy(() => {
    loadToken += 1;
    cancelAll();
    if (frame !== 0) cancelAnimationFrame(frame);
    void docTask?.destroy();
    doc = null;
    docTask = null;
  });
</script>

<div
  bind:this={scroller}
  class="absolute inset-0 overflow-auto"
  data-pdf-viewer
  {onscroll}
  role="document"
  aria-label="PDF pages"
>
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div
    class="relative mx-auto flex flex-col items-center"
    style="width: {cssWidth + SIDE * 2}px; padding: {GAP}px {SIDE}px; gap: {GAP}px"
    {onclick}
  >
    {#each { length: count } as _, index (index)}
      <div
        use:slot={index}
        class="shrink-0 overflow-hidden border border-line bg-paper"
        style="width: {cssWidth}px; height: {slotHeight}px"
        data-page={index + 1}
        role="img"
        aria-label="Page {index + 1} of {count}"
      ></div>
    {/each}
    {#if markBox !== null}
      {#key mark?.id}
        <div
          class="mng-preview-mark pointer-events-none absolute"
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
