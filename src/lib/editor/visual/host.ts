/**
 * What the visual layer needs from the app, as one small interface.
 *
 * The CodeMirror extension imports nothing from the Svelte state or the kernel
 * client: the shell hands it a `VisualHost`. That keeps the extension testable
 * with a fake host, and keeps `$lib/editor` free of cycles with `$lib/state`.
 */

import type { VisualApi } from "./client";

export type MathLanguage = "typst" | "latex";

/** Why the visual layer is not (fully) showing, for the status line. */
export type VisualStatus = "starting" | "ready" | "degraded" | "unavailable";

export interface VisualHost {
  /** Project path of the document, for its adapter and for relative images. */
  readonly path: string;
  readonly api: VisualApi;
  /** Bytes of a project file (an image), by project-relative path. */
  readBytes(path: string): Promise<Uint8Array>;
  /** The vector SVG of a formula. Rejects when the formula does not compile. */
  renderMath(language: MathLanguage, source: string, size: number): Promise<string>;
  /**
   * The project path an image source stands for, or null when it is not a
   * project file (a URL): such images are shown as source, never fetched.
   */
  resolveAsset(src: string): string | null;
  /** The `.figure` scene behind an exported `.svg`, if the project has one. */
  figureFor(svgPath: string): string | null;
  /** Open the figure studio on a scene. */
  openFigure(scenePath: string): void;
  /** Called when the state of the layer changes. */
  onStatus?(status: VisualStatus): void;
}
