<script lang="ts">
  import FigureToolbar from "./FigureToolbar.svelte";
  import FigureNodeView from "./figure/FigureNodeView.svelte";
  import FigureOverlay from "./figure/FigureOverlay.svelte";
  import FigureProperties from "./figure/FigureProperties.svelte";
  import LayersPanel from "./figure/LayersPanel.svelte";
  import FormulaEditor from "./figure/FormulaEditor.svelte";
  import MermaidDialog from "./figure/MermaidDialog.svelte";
  import { handleCursor } from "$lib/figure/handles";
  import { TOOLS } from "$lib/figure/keymap";
  import { strokeColour } from "$lib/figure/palette";
  import { isConnector, type FigureNode } from "$lib/figure/scene";
  import { cull, toScreen, viewBox, visibleRect } from "$lib/figure/viewport";
  import { focusedSurface } from "$lib/commands/surface";
  import { reportError } from "$lib/kernel";
  import { figures } from "$lib/state/figures.svelte";
  import { figureUi, GRID_STEP } from "$lib/state/figureUi.svelte";
  import { layout } from "$lib/state/layout.svelte";
  import type { FigureState, PointerInfo } from "$lib/state/figure.svelte";

  /**
   * The figure studio canvas (ADR 0012), shown inside a tab.
   *
   * An infinite SVG canvas over the scene mirror: nodes culled to the view,
   * drawn from the same primitives the export writes. Pointer work is local
   * (a preview scene); releasing a gesture sends one plugin operation. The
   * figure plugin owns the scene: edits, undo and saving (`*.figure.json`
   * plus the SVG documents embed) go through it.
   */
  interface Props {
    figure: FigureState;
  }

  const { figure }: Props = $props();

  /** Figure tokens drawn with the app theme's values (files use `print`). */
  const theme = $derived(layout.theme);

  let host: HTMLDivElement | undefined = $state();
  let surface: SVGSVGElement | undefined = $state();
  let rootWidth = $state(0);
  let hostWidth = $state(0);
  let hostHeight = $state(0);

  $effect(() => {
    figure.setSize(hostWidth, hostHeight);
  });

  /** Under this width the side panels open over the canvas instead of beside it. */
  const narrow = $derived(rootWidth > 0 && rootWidth < 860);
  /** From this width the file actions carry text labels. */
  const roomy = $derived(rootWidth >= 940);

  const view = $derived(figure.view);
  const scene = $derived(figure.display);
  const byId = $derived(new Map(scene.nodes.map((n) => [n.id, n] as const)));
  const visible = $derived(
    cull(scene.nodes, visibleRect(figure.view, hostWidth, hostHeight, 160)),
  );
  const erasing = $derived(
    figure.gesture?.kind === "erase" ? figure.gesture.ids : new Set<string>(),
  );
  const editingId = $derived(figure.editing?.id ?? null);
  /** The page shows its background (a custom one does not follow the theme). */
  const pageFill = $derived(
    figure.scene.background === undefined
      ? "none"
      : strokeColour(figure.scene.background, theme),
  );

  function target(node: FigureNode, end: "start" | "end"): FigureNode | undefined {
    if (!isConnector(node)) return undefined;
    const b = end === "start" ? node.start : node.end;
    return b === undefined ? undefined : byId.get(b.node);
  }

  const toolKeys = TOOLS.map((t) => `${t.key} ${t.label.toLowerCase()}`).join(", ");

  function screenOf(event: MouseEvent): { x: number; y: number } {
    const box = host?.getBoundingClientRect();
    return box === undefined
      ? { x: event.clientX, y: event.clientY }
      : { x: event.clientX - box.left, y: event.clientY - box.top };
  }

  function info(event: PointerEvent | MouseEvent): PointerInfo {
    const screen = screenOf(event);
    const v = figure.view;
    return {
      screen,
      point: { x: screen.x / v.zoom + v.x, y: screen.y / v.zoom + v.y },
      button: event.button,
      shift: event.shiftKey,
      alt: event.altKey,
      mod: event.ctrlKey || event.metaKey,
      pressure:
        "pointerType" in event && event.pointerType === "pen" ? event.pressure : 0.5,
    };
  }

  function onpointerdown(event: PointerEvent): void {
    // Focus is managed here: the default mousedown focus would take it back
    // from an editor this press opens (the text tool, a formula).
    event.preventDefault();
    surface?.focus();
    surface?.setPointerCapture(event.pointerId);
    figure.pointerDown(info(event));
  }

  let hoverCursor = $state<string | null>(null);

  function onpointermove(event: PointerEvent): void {
    const p = info(event);
    if (figure.gesture === null && figure.tool === "select") {
      const h = figure.handleUnder(p.point);
      hoverCursor =
        h === undefined
          ? null
          : h === "start" || h === "end"
            ? "crosshair"
            : handleCursor(h);
    }
    figure.pointerMove(p);
  }

  function onpointerup(event: PointerEvent): void {
    if (surface?.hasPointerCapture(event.pointerId)) {
      surface.releasePointerCapture(event.pointerId);
    }
    figure.pointerUp(info(event));
  }

  function ondblclick(event: MouseEvent): void {
    figure.doubleClick(info(event).point);
  }

  function onkeydown(event: KeyboardEvent): void {
    if (event.key === " ") {
      event.preventDefault();
      figure.spaceHeld = true;
      return;
    }
    if (figure.key(event)) event.preventDefault();
  }

  /**
   * Chords pressed while a properties button has focus (after clicking
   * "Align top", say) still reach the canvas; text fields keep their keys.
   */
  function hostKeydown(event: KeyboardEvent): void {
    const el = event.target;
    if (el === surface || !(el instanceof HTMLElement)) return;
    if (el.closest("input, textarea, select, [contenteditable]") !== null) return;
    const chord = event.ctrlKey || event.metaKey || event.key === "Delete";
    if (chord && figure.key(event)) event.preventDefault();
  }

  // Edit > Undo, Cut, Copy, Paste, Select all reach the canvas when it (or
  // its panels) had focus.
  $effect(() => {
    const root = host;
    if (root === undefined) return;
    const writeClipboard = (text: string | null) => {
      if (text !== null) {
        navigator.clipboard
          .writeText(text)
          .catch((e: unknown) => reportError("figure.copy", e));
      }
    };
    return focusedSurface.register({
      id: `figure:${figure.path}`,
      contains: (el) => root.parentElement?.contains(el as Node) ?? false,
      available: () => root.isConnected,
      actions: {
        undo: () => figure.undo(),
        redo: () => figure.redo(),
        copy: () => writeClipboard(figure.copyText()),
        cut: () => writeClipboard(figure.cut()),
        paste: async () => {
          try {
            figure.pasteText(await navigator.clipboard.readText());
          } catch (error) {
            reportError("figure.paste", error);
          }
        },
        selectAll: () => {
          figure.key({
            key: "a",
            ctrlKey: true,
            metaKey: false,
            shiftKey: false,
            altKey: false,
          });
        },
      },
    });
  });

  function onkeyup(event: KeyboardEvent): void {
    if (event.key === " ") figure.spaceHeld = false;
  }

  // Wheel needs a non-passive listener to stop the page from scrolling.
  $effect(() => {
    const el = surface;
    if (el === undefined) return;
    const onwheel = (event: WheelEvent) => {
      event.preventDefault();
      const scale = event.deltaMode === 1 ? 16 : 1;
      figure.wheel(
        screenOf(event),
        event.deltaX * scale,
        event.deltaY * scale,
        event.ctrlKey || event.metaKey,
      );
    };
    el.addEventListener("wheel", onwheel, { passive: false });
    return () => el.removeEventListener("wheel", onwheel);
  });

  // The laser trail fades while it has points.
  $effect(() => {
    if (figure.laser.length === 0) return;
    let frame = requestAnimationFrame(function tick(now) {
      figure.fadeLaser(now);
      if (figure.laser.length > 0) frame = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(frame);
  });

  /* ------------------------------------------------------------------ */
  /* Clipboard and drops                                                 */
  /* ------------------------------------------------------------------ */

  function focused(): boolean {
    return surface !== undefined && document.activeElement === surface;
  }

  function readImage(file: File): void {
    const reader = new FileReader();
    reader.onerror = () => reportError("figure.paste-image", reader.error);
    reader.onload = () => {
      const uri = typeof reader.result === "string" ? reader.result : "";
      if (file.size > 512 * 1024) {
        figure.pasteImage(uri, file.size, 1, 1);
        return;
      }
      const img = new Image();
      img.onload = () =>
        figure.pasteImage(uri, file.size, img.naturalWidth, img.naturalHeight);
      img.onerror = () => figure.pasteImage(uri, file.size, 240, 160);
      img.src = uri;
    };
    reader.readAsDataURL(file);
  }

  $effect(() => {
    const oncopy = (event: ClipboardEvent) => {
      if (!focused()) return;
      const text = event.type === "cut" ? figure.cut() : figure.copyText();
      if (text === null) return;
      event.clipboardData?.setData("text/plain", text);
      event.preventDefault();
    };
    const onpaste = (event: ClipboardEvent) => {
      if (!focused()) return;
      const data = event.clipboardData;
      if (data === null) return;
      const image = [...data.files].find((f) => f.type.startsWith("image/"));
      event.preventDefault();
      if (image !== undefined) {
        readImage(image);
        return;
      }
      figure.pasteText(data.getData("text/plain"));
    };
    window.addEventListener("copy", oncopy);
    window.addEventListener("cut", oncopy);
    window.addEventListener("paste", onpaste);
    return () => {
      window.removeEventListener("copy", oncopy);
      window.removeEventListener("cut", oncopy);
      window.removeEventListener("paste", onpaste);
    };
  });

  function ondrop(event: DragEvent): void {
    const files = [...(event.dataTransfer?.files ?? [])].filter((f) =>
      f.type.startsWith("image/"),
    );
    if (files.length === 0) return;
    event.preventDefault();
    for (const f of files) readImage(f);
  }

  /* ------------------------------------------------------------------ */
  /* Label editor                                                        */
  /* ------------------------------------------------------------------ */

  const editorBox = $derived.by(() => {
    const e = figure.editing;
    if (e === null) return null;
    const tl = toScreen(figure.view, { x: e.box.x, y: e.box.y });
    const z = figure.view.zoom;
    const node = e.id === null ? undefined : byId.get(e.id);
    const size = (node?.style?.fontSize ?? 16) * z;
    return {
      left: tl.x,
      top: tl.y,
      width: Math.max(e.box.width * z, 60),
      height: Math.max(e.box.height * z, size * 1.6),
      size,
      angle: e.angle,
      align: e.kind === "text" ? "left" : "center",
      mono: node?.style?.fontFamily === "mono",
    };
  });

  function labelKeydown(event: KeyboardEvent): void {
    event.stopPropagation();
    if (event.key === "Escape") {
      event.preventDefault();
      figure.cancelLabel();
      surface?.focus();
    } else if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      figure.commitLabel();
      surface?.focus();
    }
  }

  function focusOnMount(el: HTMLTextAreaElement): void {
    el.focus();
    el.select();
  }

  const cursor = $derived(
    figure.gesture?.kind === "pan"
      ? "grabbing"
      : figure.tool === "hand" || figure.spaceHeld
        ? "grab"
        : figure.tool === "select"
          ? (hoverCursor ?? (figure.gesture?.kind === "move" ? "move" : "default"))
          : figure.tool === "text"
            ? "text"
            : "crosshair",
  );
</script>

<div
  class="flex h-full min-h-0 w-full min-w-0 flex-1 flex-col bg-bg"
  bind:clientWidth={rootWidth}
>
  <FigureToolbar
    {figure}
    {narrow}
    {roomy}
    onsave={() => void figures.save(figure.path)}
  />

  <div class="relative flex min-h-0 flex-1" onkeydown={hostKeydown} role="presentation">
    <LayersPanel {figure} overlay={narrow} />
    <div
      bind:this={host}
      bind:clientWidth={hostWidth}
      bind:clientHeight={hostHeight}
      class="relative min-h-0 min-w-0 flex-1 overflow-hidden"
      role="presentation"
      ondragover={(e) => e.preventDefault()}
      {ondrop}
    >
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions, a11y_no_noninteractive_tabindex -->
      <svg
        bind:this={surface}
        role="application"
        tabindex="0"
        aria-label="Figure canvas. Tool keys: {toolKeys}. Drag to draw or select; double-click a shape to edit its label. Space-drag, the middle button or scrolling pans; Ctrl+scroll zooms. Delete removes the selection. Ctrl+Z undoes."
        viewBox={viewBox(figure.view, hostWidth, hostHeight)}
        width="100%"
        height="100%"
        preserveAspectRatio="xMinYMin meet"
        class="block touch-none outline-none select-none"
        style="cursor: {cursor}"
        {onpointerdown}
        {onpointermove}
        {onpointerup}
        onpointercancel={() => figure.cancel()}
        {onkeydown}
        {onkeyup}
        onblur={() => (figure.spaceHeld = false)}
        {ondblclick}
      >
        <!-- The page: what an export covers when nothing lies outside it. -->
        <rect
          data-page
          x="0"
          y="0"
          width={figure.scene.width}
          height={figure.scene.height}
          fill={pageFill}
          stroke="var(--mng-line)"
          stroke-width="1"
          vector-effect="non-scaling-stroke"
        />

        {#if figureUi.grid && view.zoom * GRID_STEP >= 6}
          <defs>
            <pattern
              id="figure-grid"
              width={GRID_STEP}
              height={GRID_STEP}
              patternUnits="userSpaceOnUse"
            >
              <path
                d="M{GRID_STEP} 0H0V{GRID_STEP}"
                fill="none"
                stroke="var(--mng-line)"
                stroke-width={1 / view.zoom}
              />
            </pattern>
          </defs>
          <rect
            data-grid
            x="0"
            y="0"
            width={figure.scene.width}
            height={figure.scene.height}
            fill="url(#figure-grid)"
            pointer-events="none"
          />
        {/if}

        {#each visible as node (node.id)}
          <FigureNodeView
            {node}
            startTarget={target(node, "start")}
            endTarget={target(node, "end")}
            {theme}
            dim={erasing.has(node.id)}
            hideText={editingId === node.id}
            formula={figure.formulaShown}
          />
        {/each}

        <FigureOverlay {figure} />
      </svg>

      {#if figure.editing !== null && editorBox !== null}
        <textarea
          use:focusOnMount
          class="selectable absolute z-10 resize-none overflow-hidden border border-accent bg-transparent p-0 text-text outline-none"
          style="left: {editorBox.left}px; top: {editorBox.top}px; width: {editorBox.width}px; height: {editorBox.height}px; font-size: {editorBox.size}px; line-height: 1.25; text-align: {editorBox.align}; font-family: {editorBox.mono
            ? 'var(--font-mono)'
            : 'var(--font-sans)'}; transform: rotate({editorBox.angle}deg); transform-origin: center;"
          aria-label="Edit label"
          data-label-editor
          value={figure.editing.text}
          oninput={(e) =>
            figure.setEditingText((e.currentTarget as HTMLTextAreaElement).value)}
          onblur={() => figure.commitLabel()}
          onkeydown={labelKeydown}
        ></textarea>
      {/if}

      <FormulaEditor {figure} {hostWidth} {hostHeight} />

      {#if figure.mermaidPrompt !== null}
        <div
          class="absolute bottom-8 left-1/2 z-10 flex -translate-x-1/2 items-center gap-1.5 border border-line bg-surface px-2 py-1.5 text-[12px] text-text-60"
          data-mermaid-prompt
          role="group"
          aria-label="Pasted Mermaid text"
        >
          Mermaid text:
          <button
            type="button"
            class="mng-btn h-7 px-2"
            onclick={() =>
              void figure.insertMermaid(figure.mermaidPrompt ?? "", "shapes")}
            >Insert as shapes</button
          >
          <button
            type="button"
            class="mng-btn h-7 px-2"
            onclick={() =>
              void figure.insertMermaid(figure.mermaidPrompt ?? "", "block")}
            >Insert as block</button
          >
          <button
            type="button"
            class="mng-btn h-7 px-2"
            onclick={() => figure.pasteAsText(figure.mermaidPrompt ?? "")}
            >Paste as text</button
          >
        </div>
      {/if}

      {#if figure.mermaidDialog}
        <MermaidDialog {figure} />
      {/if}
    </div>

    <FigureProperties {figure} {theme} overlay={narrow} />
  </div>
</div>
