<script lang="ts" module>
  /**
   * The workbench's whole icon set: 16-unit grid, 1.25 stroke, butt caps and
   * mitred joins. Squares, rules and diagonals, never circles, so the set reads
   * as the same family as the Manang mark. Kept inline on purpose: no icon pack.
   */
  const PATHS = {
    files: "M2.5 3.5h4l1.5 1.5h5.5v8h-11z",
    "folder-open": "M2.5 3.5h4l1.5 1.5h5.5v2.5 M2.5 13.5l2-6h10l-2 6z",
    search: "M2.5 2.5h7v7h-7z M9.5 9.5l4 4",
    figures: "M3 13l1-4 7-7 3 3-7 7z M9.5 3.5l3 3",
    references: "M6 2.5H3.5v11H6 M10 2.5h2.5v11H10 M6.5 6h3 M6.5 8.5h3 M6.5 11h3",
    plugins: "M2.5 9h5v5h-5z M8.5 9h5v5h-5z M5.5 2h5v5h-5z",
    agent: "M2.5 2.5h11v8h-5.5l-3 3v-3h-2.5z M5.5 5.5h5 M5.5 7.5h3",
    settings: "M2 5h12 M2 11h12 M10 3v4 M5 9v4",
    close: "M3.5 3.5l9 9 M12.5 3.5l-9 9",
    plus: "M8 3v10 M3 8h10",
    "chevron-right": "M6 3.5l4.5 4.5L6 12.5",
    "chevron-down": "M3.5 6l4.5 4.5L12.5 6",
    file: "M3.5 2h6l3 3v9h-9z M9.5 2v3h3",
    "file-plus": "M3.5 2h6l3 3v9h-9z M8 7.5v4 M6 9.5h4",
    "folder-plus": "M2.5 3.5h4l1.5 1.5h5.5v8h-11z M8 7.5v4 M6 9.5h4",
    refresh: "M3 8a5 5 0 0 1 9-3 M12 2v3H9 M13 8a5 5 0 0 1-9 3 M4 14v-3h3",
    "collapse-all": "M4 6l4-3 4 3 M4 10l4 3 4-3",
    split: "M2 3h12v10H2z M8 3v10",
    editor: "M2 3h12v10H2z M5 6.5h6 M5 9.5h4",
    preview: "M2 3h12v10H2z M5.5 5.5h5v5h-5z",
    export: "M8 2v8 M4.5 6.5L8 10l3.5-3.5 M3 13.5h10",
    zen: "M2 6V2h4 M10 2h4v4 M14 10v4h-4 M6 14H2v-4",
    maximise: "M2.5 2.5h11v11h-11z M6 10l4-4 M6.5 6H10v3.5",
    error: "M2.5 2.5h11v11h-11z M5.5 5.5l5 5 M10.5 5.5l-5 5",
    warning: "M8 2.5l6 11H2z M8 6.5v3.5 M8 11.5v.5",
    check: "M3 8.5l3.5 3.5L13 4.5",
    invert: "M2.5 2.5h11v11h-11z M8 2.5v11 M8 6l5.5-3.5 M8 10l5.5-3.5 M8 13.5l5.5-3.5",
    branch: "M4 2.5h2v2H4z M4 11.5h2v2H4z M10 5.5h2v2h-2z M5 4.5v7 M5 9h5V7.5",
    "source-control":
      "M4 2.5h2v2H4z M4 11.5h2v2H4z M10 6.5h2v2h-2z M5 4.5v7 M5 9.5h4v-1",
    outline:
      "M2.5 3.5h2v2h-2z M6.5 4.5h7 M4 7.5h2v2H4z M8 8.5h5.5 M5.5 11.5h2v2h-2z M9.5 12.5h4",
    present: "M4 3l9 5-9 5z",
    slides: "M2 3h12v8H2z M6 14h4 M8 11v3",
    trash: "M3.5 4.5h9 M6 4.5V3h4v1.5 M4.5 4.5l.5 9h6l.5-9",
    panel: "M2 3h12v10H2z M2 9.5h12",
    sidebar: "M2 3h12v10H2z M6 3v10",
    command: "M2.5 3.5h11v9h-11z M5 7l2 1.5L5 10 M8.5 10h2.5",
    copy: "M5.5 5.5h8v8h-8z M2.5 10.5v-8h8",
    arrowUp: "M8 13V3 M4 7l4-4 4 4",
    arrowDown: "M8 3v10 M4 9l4 4 4-4",
    select: "M3.5 2.5v10l3-2.5 2 3.5 1.5-1-2-3.5h4z",
    freehand: "M2.5 12c2-5 4-6 5-3s3 1 6-5",
    rect: "M2.5 4.5h11v7h-11z",
    ellipse: "M5.5 3.5h5l3 3v3l-3 3h-5l-3-3v-3z",
    undo: "M6 4L2.5 7.5 6 11 M2.5 7.5h8v5",
    redo: "M10 4l3.5 3.5L10 11 M13.5 7.5h-8v5",
    arrow: "M2.5 8h10 M9 4.5L12.5 8 9 11.5",
    formula: "M3 3.5h10 M3 3.5l5 4.5-5 4.5h10",
    save: "M2.5 2.5h9l2 2v9h-11z M5 2.5v4h5v-4 M5 13.5v-4h6v4",
    clear: "M3 8h10",
    home: "M2.5 7.5L8 2.5l5.5 5 M4 6.5v7h8v-7 M7 13.5v-4h2v4",
    pin: "M5.5 2.5h5v4l1.5 2.5H4l1.5-2.5z M8 9v4.5",
    "pin-filled": "M5.5 2.5h5v4l1.5 2.5H4l1.5-2.5z M6.5 4h3 M6.5 5.5h3 M8 9v4.5",
    more: "M2.5 7h2v2h-2z M7 7h2v2H7z M11.5 7h2v2h-2z",
    grid: "M2.5 2.5h4.5v4.5H2.5z M9 2.5h4.5v4.5H9z M2.5 9h4.5v4.5H2.5z M9 9h4.5v4.5H9z",
    list: "M2.5 4h11 M2.5 8h11 M2.5 12h11",
    archive: "M2.5 3.5h11v3h-11z M3.5 6.5v7h9v-7 M6.5 9h3",
    recent: "M2.5 2.5h11v11h-11z M8 5v3.5l2.5 1.5",
    templates: "M2.5 2.5h5v5h-5z M9 2.5h4.5v5H9z M2.5 9h11v4.5h-11z",
    "window-minimize": "M3.5 8.5h9",
    "window-maximize": "M3.5 3.5h9v9h-9z",
    "window-restore": "M3.5 5.5h7v7h-7z M5.5 5.5v-2h7v7h-2",
    "window-close": "M4 4l8 8 M12 4l-8 8",
    external: "M9 2.5h4.5V7 M13.5 2.5L7.5 8.5 M11.5 9.5v4h-9v-9h4",
    hand: "M5 9V4h1.5v4 M6.5 7.5V3H8v4.5 M8 7.5V3.5h1.5v4.5 M9.5 8V5H11v5l-1.5 3.5h-4L3 10.5 4 9.5l1 1",
    diamond: "M8 2.5L13.5 8 8 13.5 2.5 8z",
    line: "M3 13L13 3",
    text: "M3.5 3.5h9 M8 3.5v9 M6 12.5h4",
    frame: "M4.5 2v12 M11.5 2v12 M2 4.5h12 M2 11.5h12",
    eraser: "M2.5 13.5h11 M5 12l-2-2 6.5-6.5 3 3L7 12z M6.5 6l3 3",
    laser: "M2.5 13.5l6-6 M10.5 2v2.5 M14 5.5h-2.5 M13 3l-1.5 1.5 M8.5 5.5l2 2",
    mermaid: "M2.5 2.5h4v3h-4z M9.5 10.5h4v3h-4z M4.5 5.5v6h5",
    fit: "M2 5V2h3 M11 2h3v3 M14 11v3h-3 M5 14H2v-3 M5.5 5.5h5v5h-5z",
    "zoom-in": "M2.5 2.5h7v7h-7z M9.5 9.5l4 4 M6 4v4 M4 6h4",
    "zoom-out": "M2.5 2.5h7v7h-7z M9.5 9.5l4 4 M4 6h4",
    group: "M2.5 2.5h7v7h-7z M6.5 6.5h7v7h-7z",
    ungroup: "M2.5 2.5h5v5h-5z M8.5 8.5h5v5h-5z",
    lock: "M3.5 7.5h9v6h-9z M5.5 7.5v-3h5v3",
    unlock: "M3.5 7.5h9v6h-9z M5.5 7.5v-3h5",
    "align-left": "M2.5 2v12 M4.5 4.5h8v3h-8z M4.5 9.5h5v3h-5z",
    "align-center": "M8 2v12 M3.5 4.5h9v3h-9z M5 9.5h6v3H5z",
    "align-right": "M13.5 2v12 M3.5 4.5h8v3h-8z M6.5 9.5h5v3h-5z",
    "align-top": "M2 2.5h12 M4.5 4.5h3v8h-3z M9.5 4.5h3v5h-3z",
    "align-middle": "M2 8h12 M4.5 3.5h3v9h-3z M9.5 5h3v6h-3z",
    "align-bottom": "M2 13.5h12 M4.5 3.5h3v8h-3z M9.5 6.5h3v5h-3z",
    "distribute-h": "M2.5 2v12 M13.5 2v12 M6.5 5h3v6h-3z",
    "distribute-v": "M2 2.5h12 M2 13.5h12 M5 6.5h6v3H5z",
    "to-front": "M8 13V5 M4.5 8.5L8 5l3.5 3.5 M3 2.5h10",
    "to-back": "M8 3v8 M4.5 7.5L8 11l3.5-3.5 M3 13.5h10",
    ratio: "M3 3.5h10v9H3z M3 12.5L13 3.5",
    eye: "M1.5 8l3-3.5h7L14.5 8l-3 3.5h-7z M7 7h2v2H7z",
    "eye-off": "M1.5 8l3-3.5h7L14.5 8l-3 3.5h-7z M2.5 13.5l11-11",
    layers: "M8 2.5l6 3-6 3-6-3z M2 8.5l6 3 6-3 M2 11.5l6 3 6-3",
  } as const;

  export type IconName = keyof typeof PATHS;
</script>

<script lang="ts">
  interface Props {
    name: IconName;
    size?: number;
    class?: string;
  }

  const { name, size = 16, class: className = "" }: Props = $props();
</script>

<svg
  viewBox="0 0 16 16"
  width={size}
  height={size}
  fill="none"
  stroke="currentColor"
  stroke-width="1.25"
  stroke-linecap="butt"
  stroke-linejoin="miter"
  class={className}
  aria-hidden="true"
  focusable="false"
>
  <path d={PATHS[name]} />
</svg>
