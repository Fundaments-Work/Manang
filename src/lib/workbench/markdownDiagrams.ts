/**
 * Diagram fences in the Markdown preview. `markdown.render-html` (sanitised in
 * Rust) leaves an empty `<figure class="mng-diagram" data-diagram="<key>">`
 * for each fence and lists the fences; this module renders each fence through
 * `diagram.render` (in the theme of the paper it sits on), keeps the result as a Blob URL, and
 * fills the placeholder with an `<img>` (never the SVG markup itself) or a
 * quiet error box. A fence whose language, source and theme did not change
 * keeps its picture without a new render.
 *
 * Plain TypeScript with injected kernel calls and URL factory, so the rules
 * are unit-tested; `state/build.svelte.ts` drives it.
 */

import { errorMessage } from "$lib/kernel/client";
import type { DiagramApi } from "$lib/kernel/api";
import { sniffArtifact, type UrlApi } from "$lib/kernel/artifact";
import type { DiagramDiagnostic, DiagramTheme, MarkdownFence } from "$lib/kernel/types";

import { DIAGRAM_NAMES, diagramErrorText, parseDiagramError } from "./diagram";
import { fromDiagram, type Problem } from "./problems";

/**
 * The theme of fences in the Markdown preview. The preview page is paper in
 * every app theme (paper-white, ink-black, as it prints; see `app.css`), so a
 * dark-theme diagram (light ink, transparent ground) would vanish on it. The
 * light theme's ink on a transparent ground matches the page.
 */
export const PAPER_DIAGRAM_THEME: DiagramTheme = "light";

/** What one fence turned into. */
export interface FenceResult {
  /** Blob URL of the SVG, or null when the fence has an error. */
  readonly url: string | null;
  /** One sentence about the error, or "". */
  readonly error: string;
  /** `diagram.check` findings, lines relative to the fence's source. */
  readonly diagnostics: readonly DiagramDiagnostic[];
}

export interface FenceRendererDeps {
  readonly api: Pick<DiagramApi, "check" | "render">;
  readonly urls: UrlApi;
  /** Failures that are not the diagram's fault (kernel trouble). */
  readonly onError?: (error: unknown) => void;
}

const cacheKey = (theme: DiagramTheme, fence: MarkdownFence): string =>
  `${theme}\u0000${fence.language}\u0000${fence.source}`;

/** Renders fences, keeping one Blob URL per distinct (theme, language, source). */
export class FenceRenderer {
  readonly #deps: FenceRendererDeps;
  readonly #cache = new Map<string, Promise<FenceResult>>();
  #generation = 0;

  constructor(deps: FenceRendererDeps) {
    this.#deps = deps;
  }

  /**
   * Results for `fences` keyed by fence key. Unchanged fences reuse their
   * picture; pictures of fences that are gone are released.
   */
  async prepare(
    fences: readonly MarkdownFence[],
    theme: DiagramTheme,
  ): Promise<Map<string, FenceResult>> {
    const generation = ++this.#generation;
    const wanted = new Set<string>();
    const pending = fences.map(async (fence) => {
      const key = cacheKey(theme, fence);
      wanted.add(key);
      let result = this.#cache.get(key);
      if (result === undefined) {
        result = this.#render(fence, theme);
        this.#cache.set(key, result);
      }
      return [fence.key, await result] as const;
    });
    const results = new Map(await Promise.all(pending));
    // Only the newest call prunes, so a slower older one cannot revoke
    // pictures a newer one still shows.
    if (generation === this.#generation) await this.#prune(wanted);
    return results;
  }

  /** Revoke every picture (the preview closed). */
  async release(): Promise<void> {
    this.#generation += 1;
    await this.#prune(new Set());
  }

  async #prune(keep: ReadonlySet<string>): Promise<void> {
    for (const [key, result] of [...this.#cache]) {
      if (keep.has(key)) continue;
      this.#cache.delete(key);
      const { url } = await result;
      if (url !== null) this.#deps.urls.revoke(url);
    }
  }

  async #render(fence: MarkdownFence, theme: DiagramTheme): Promise<FenceResult> {
    const [checked, rendered] = await Promise.allSettled([
      this.#deps.api.check(fence.language, fence.source),
      this.#deps.api.render(fence.language, fence.source, theme),
    ]);
    const diagnostics =
      checked.status === "fulfilled" ? [...checked.value.diagnostics] : [];
    if (checked.status === "rejected") this.#deps.onError?.(checked.reason);
    if (rendered.status === "fulfilled") {
      const bytes = rendered.value;
      if (sniffArtifact(bytes)?.kind !== "svg") {
        return {
          url: null,
          error: "The diagram plugin returned something that is not an SVG.",
          diagnostics,
        };
      }
      // Copy into a plain ArrayBuffer view so the Blob never aliases IPC memory.
      const url = this.#deps.urls.create(
        new Blob([new Uint8Array(bytes)], { type: "image/svg+xml" }),
      );
      return { url, error: "", diagnostics };
    }
    const message = errorMessage(rendered.reason);
    const position = parseDiagramError(message);
    if (position === null) this.#deps.onError?.(rendered.reason);
    if (position !== null && !diagnostics.some((d) => d.severity === "error")) {
      diagnostics.push(position);
    }
    return { url: null, error: diagramErrorText(message), diagnostics };
  }
}

function escapeHtml(text: string): string {
  return text
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

/** Exactly what the Rust renderer emits for a fence; nothing else matches. */
const PLACEHOLDER =
  /<figure class="mng-diagram" data-diagram="([0-9a-f]{16})" data-language="(mermaid|pikchr|svgbob)" data-line="(\d+)"><\/figure>/g;

/**
 * The preview HTML with each placeholder filled: an `<img>` of the fence's
 * picture, a quiet error box, or (not rendered yet) left empty. `html` comes
 * sanitised from Rust; everything added here is escaped.
 */
export function fillPlaceholders(
  html: string,
  results: ReadonlyMap<string, FenceResult>,
): string {
  return html.replace(
    PLACEHOLDER,
    (whole, key: string, language: keyof typeof DIAGRAM_NAMES, line: string) => {
      const result = results.get(key);
      if (result === undefined) return whole;
      const open = `<figure class="mng-diagram" data-diagram="${key}" data-language="${language}" data-line="${line}"`;
      const name = DIAGRAM_NAMES[language];
      if (result.url !== null) {
        return `${open}><img src="${escapeHtml(result.url)}" alt="${escapeHtml(name)} diagram (line ${line})" data-diagram-fence></figure>`;
      }
      return `${open} data-diagram-error><p>${escapeHtml(name)} diagram, line ${line}: ${escapeHtml(result.error)}</p></figure>`;
    },
  );
}

/**
 * The fences' diagnostics as Problems of `file`, moved from fence lines to
 * document lines (a diagnostic at line n of a fence opened at line L is at
 * line L + n).
 */
export function fenceProblems(
  file: string,
  fences: readonly MarkdownFence[],
  results: ReadonlyMap<string, FenceResult>,
): Problem[] {
  const shifted: DiagramDiagnostic[] = [];
  for (const fence of fences) {
    for (const d of results.get(fence.key)?.diagnostics ?? []) {
      shifted.push({
        ...d,
        line: d.line + fence.line,
        endLine: d.endLine + fence.line,
      });
    }
  }
  return fromDiagram(file, shifted);
}
