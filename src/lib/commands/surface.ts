/**
 * The focused surface: where Edit > Undo, Copy, Find and friends go.
 *
 * Edit commands are not tied to one component. Whatever has focus handles
 * them: the CodeMirror editor, the figure canvas, the PDF viewer's find, or a
 * plain text field (natively). Each surface registers once with what it can
 * do; the dispatcher picks the one that contains the focused element, or the
 * one focused last (a menu click moves focus away for a moment, and Find with
 * nothing focused should still reach the open document).
 *
 * Registering:
 *
 * ```ts
 * const stop = focusedSurface.register({
 *   id: "pdf",
 *   contains: (el) => viewer.contains(el),
 *   actions: { copy: () => ..., find: () => openFind() },
 * });
 * ```
 *
 * A `fallback` surface (native text fields) is only used when no other
 * surface contains the focus, since the editor's content is editable too.
 */

export type EditAction =
  "undo" | "redo" | "cut" | "copy" | "paste" | "selectAll" | "find" | "replace";

/** The parts of a DOM element the dispatcher needs; tests pass plain objects. */
export type FocusTarget = object;

export interface FocusSurface {
  readonly id: string;
  /** Whether the element is inside this surface. */
  contains(element: FocusTarget): boolean;
  /** False when the surface is gone (its view unmounted); true when absent. */
  available?(): boolean;
  readonly actions: Partial<Record<EditAction, () => void | Promise<void>>>;
  /** Only used when no other surface contains the focus. */
  readonly fallback?: boolean;
}

export class SurfaceRegistry {
  #surfaces: FocusSurface[] = [];
  #last: FocusSurface | null = null;

  /** Add a surface; returns the function that removes it. */
  register(surface: FocusSurface): () => void {
    this.#surfaces = [...this.#surfaces.filter((s) => s.id !== surface.id), surface];
    return () => {
      this.#surfaces = this.#surfaces.filter((s) => s !== surface);
      if (this.#last === surface) this.#last = null;
    };
  }

  #live(surface: FocusSurface): boolean {
    return surface.available?.() ?? true;
  }

  /** The surface containing `element`: specific surfaces first, then fallbacks. */
  #containing(element: FocusTarget | null): FocusSurface | null {
    if (element === null) return null;
    const live = this.#surfaces.filter((s) => this.#live(s));
    return (
      live.find((s) => s.fallback !== true && s.contains(element)) ??
      live.find((s) => s.fallback === true && s.contains(element)) ??
      null
    );
  }

  /** Remember the surface focus last entered (call on `focusin`). */
  noteFocus(element: FocusTarget | null): void {
    const surface = this.#containing(element);
    if (surface !== null) this.#last = surface;
  }

  /**
   * Who handles an action for this focused element: the surface containing
   * it, else the surface focused last (if it can do the action and is still
   * there). Native fields are only targeted while focused.
   */
  target(action: EditAction, element: FocusTarget | null): FocusSurface | null {
    const here = this.#containing(element);
    if (here !== null) return here.actions[action] !== undefined ? here : null;
    const last = this.#last;
    if (
      last !== null &&
      last.fallback !== true &&
      this.#surfaces.includes(last) &&
      this.#live(last) &&
      last.actions[action] !== undefined
    )
      return last;
    return null;
  }

  can(action: EditAction, element: FocusTarget | null): boolean {
    return this.target(action, element) !== null;
  }

  /** Run the action on its surface. False when nothing could take it. */
  async dispatch(action: EditAction, element: FocusTarget | null): Promise<boolean> {
    const run = this.target(action, element)?.actions[action];
    if (run === undefined) return false;
    await run();
    return true;
  }
}

/** The app's one registry. */
export const focusedSurface = new SurfaceRegistry();
