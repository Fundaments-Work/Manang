/**
 * Per-user choices of the figure studio's panels, remembered in
 * `localStorage` (never in a figure file): whether the layers panel is open,
 * which inspector sections are collapsed, whether aspect is locked, and the
 * snapping and grid helpers. Storage that is missing or blocked just means
 * the choices are not remembered.
 */

const KEY = "manang.figure.ui";

interface Saved {
  layers?: boolean;
  collapsed?: string[];
  lockAspect?: boolean;
  snap?: boolean;
  grid?: boolean;
}

function load(): Saved {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw === null) return {};
    const parsed: unknown = JSON.parse(raw);
    return typeof parsed === "object" && parsed !== null ? (parsed as Saved) : {};
  } catch {
    return {};
  }
}

/** Grid spacing in scene units. */
export const GRID_STEP = 20;

class FigureUi {
  /** The layers panel is open (wide windows); overlays start closed. */
  layersOpen = $state(false);
  /** Narrow windows: the inspector overlay is open. */
  inspectorOverlay = $state(false);
  /** Narrow windows: the layers overlay is open. */
  layersOverlay = $state(false);
  collapsed = $state.raw<readonly string[]>([]);
  lockAspect = $state(false);
  /** Snap to the edges and centres of other shapes while dragging. */
  snap = $state(true);
  /** Draw a grid behind the page. */
  grid = $state(false);

  constructor() {
    const saved = load();
    this.layersOpen = saved.layers ?? true;
    this.collapsed = saved.collapsed ?? [];
    this.lockAspect = saved.lockAspect ?? false;
    this.snap = saved.snap ?? true;
    this.grid = saved.grid ?? false;
  }

  #save(): void {
    try {
      const data: Saved = {
        layers: this.layersOpen,
        collapsed: [...this.collapsed],
        lockAspect: this.lockAspect,
        snap: this.snap,
        grid: this.grid,
      };
      localStorage.setItem(KEY, JSON.stringify(data));
    } catch {
      // Blocked storage: the choice just is not remembered.
    }
  }

  isCollapsed(section: string): boolean {
    return this.collapsed.includes(section);
  }

  toggleSection(section: string): void {
    this.collapsed = this.isCollapsed(section)
      ? this.collapsed.filter((s) => s !== section)
      : [...this.collapsed, section];
    this.#save();
  }

  setLayersOpen(open: boolean): void {
    this.layersOpen = open;
    this.#save();
  }

  setLockAspect(on: boolean): void {
    this.lockAspect = on;
    this.#save();
  }

  setSnap(on: boolean): void {
    this.snap = on;
    this.#save();
  }

  setGrid(on: boolean): void {
    this.grid = on;
    this.#save();
  }
}

export const figureUi = new FigureUi();
