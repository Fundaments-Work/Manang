/**
 * Workbench layout state: which panels are open and how big, the preview
 * layout, zen mode, theme and the viewport width the responsive rules read.
 *
 * The rules live in `workbench/layout.ts`; this class holds the reactive copy
 * and persists it. Stand-in persistence: the settings plugin will take over.
 */

import {
  DEFAULT_LAYOUT,
  clampPanel,
  clampSide,
  clampSplit,
  cyclePreview,
  effectivePreview,
  isNarrow,
  revealPreview,
  sanitizeLayout,
  showPanel,
  showSide,
  toggleMaximisedPreview,
  toggleSide,
  type LayoutModel,
  type PanelTab,
  type PreviewMode,
  type SideView,
  type Theme,
} from "$lib/workbench/layout";
import { LAYOUT_KEY, loadJson, saveJson } from "$lib/workbench/persist";

export class LayoutState {
  #model = $state<LayoutModel>(DEFAULT_LAYOUT);
  /** Zen mode hides all chrome; it is never persisted. */
  zen = $state(false);
  width = $state(1280);

  get sideView(): SideView | null {
    return this.#model.sideView;
  }
  get sideWidth(): number {
    return this.#model.sideWidth;
  }
  get panelOpen(): boolean {
    return this.#model.panelOpen;
  }
  get panelHeight(): number {
    return this.#model.panelHeight;
  }
  get panelTab(): PanelTab {
    return this.#model.panelTab;
  }
  get preview(): PreviewMode {
    return this.#model.preview;
  }
  get splitRatio(): number {
    return this.#model.splitRatio;
  }
  get theme(): Theme {
    return this.#model.theme;
  }

  /** The preview layout after the narrow-window rule is applied. */
  readonly effectivePreview: PreviewMode = $derived(
    effectivePreview(this.#model.preview, this.width),
  );
  readonly narrow: boolean = $derived(isNarrow(this.width));

  /** Restore the persisted layout. Call once at start-up. */
  restore(): void {
    this.#model = sanitizeLayout(loadJson(LAYOUT_KEY));
    this.applyTheme();
  }

  #set(next: LayoutModel): void {
    this.#model = next;
    saveJson(LAYOUT_KEY, next);
  }

  toggleSide(view: SideView): void {
    this.#set(toggleSide(this.#model, view));
  }
  showSide(view: SideView): void {
    this.#set(showSide(this.#model, view));
  }
  closeSide(): void {
    this.#set({ ...this.#model, sideView: null });
  }
  setSideWidth(width: number): void {
    this.#set({ ...this.#model, sideWidth: clampSide(width) });
  }
  togglePanel(): void {
    this.#set({ ...this.#model, panelOpen: !this.#model.panelOpen });
  }
  showPanel(tab: PanelTab): void {
    this.#set(showPanel(this.#model, tab));
  }
  setPanelTab(tab: PanelTab): void {
    this.#set({ ...this.#model, panelTab: tab });
  }
  closePanel(): void {
    this.#set({ ...this.#model, panelOpen: false });
  }
  setPanelHeight(height: number): void {
    this.#set({ ...this.#model, panelHeight: clampPanel(height) });
  }
  setPreview(mode: PreviewMode): void {
    this.#set({ ...this.#model, preview: mode });
  }
  /** Preview header: preview only from split, back to split from there. */
  toggleMaximisedPreview(): void {
    this.setPreview(toggleMaximisedPreview(this.effectivePreview));
  }
  /** Preview header "Close preview": the editor alone. */
  closePreview(): void {
    this.setPreview("editor");
  }
  /** Tab strip "Show preview": split, or the preview alone in a narrow window. */
  showPreview(): void {
    this.setPreview(revealPreview(this.width));
  }
  cyclePreview(): void {
    this.setPreview(cyclePreview(this.#model.preview));
  }
  setSplitRatio(ratio: number): void {
    this.#set({ ...this.#model, splitRatio: clampSplit(ratio) });
  }
  setTheme(theme: Theme): void {
    this.#set({ ...this.#model, theme });
    this.applyTheme();
  }
  toggleZen(): void {
    this.zen = !this.zen;
  }
  reset(): void {
    this.#set(DEFAULT_LAYOUT);
    this.applyTheme();
  }

  applyTheme(): void {
    const root = document.documentElement;
    root.classList.remove("theme-dark", "theme-light");
    root.classList.add(`theme-${this.#model.theme}`);
  }
}

export const layout = new LayoutState();
