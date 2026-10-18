/**
 * Text-layer width calibration for WebKit. pdf.js sizes each run with
 * `measureText` on a canvas, but WebKitGTK lays out the span with slightly
 * different font metrics, so long runs end a few percent off the glyphs on
 * the canvas and a selection drifts away from the text. Measure what the
 * spans really are against what the PDF says, per font, and correct their
 * `scaleX`.
 *
 * Adapted from Theorem (`calibrateWebKitTextLayerWidth` in
 * `pdfjs-engine.tsx`, MIT, same owner). The statistics are pure and tested;
 * `calibrateSpans` applies them.
 */

export interface WidthSample {
  /** Font family of the span (correction is per font). */
  readonly font: string;
  /** Width the PDF gives the run, in CSS px at the current scale. */
  readonly expected: number;
  /** Width the span has on screen. */
  readonly actual: number;
}

export interface Corrections {
  readonly byFont: ReadonlyMap<string, number>;
  /** For fonts with too few samples of their own; null when not needed. */
  readonly global: number | null;
}

const MIN_SAMPLES = 24;
const MIN_FONT_SAMPLES = 8;
/** Below 2% the eye does not see it; above 25% the sample is wrong. */
const NEGLIGIBLE = 0.02;
const MAX_CORRECTION = 0.25;

function median(values: readonly number[]): number | null {
  if (values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const mid = sorted.length >> 1;
  return sorted.length % 2 === 1
    ? (sorted[mid] ?? null)
    : ((sorted[mid - 1] ?? 0) + (sorted[mid] ?? 0)) / 2;
}

function clampCorrection(ratio: number): number {
  return Math.max(1 - MAX_CORRECTION, Math.min(1 + MAX_CORRECTION, ratio));
}

/** Per-font horizontal corrections, or none when the layer already fits. */
export function corrections(samples: readonly WidthSample[]): Corrections {
  const all: number[] = [];
  const perFont = new Map<string, number[]>();
  for (const s of samples) {
    if (s.actual <= 0.01 || s.expected <= 0.01) continue;
    const ratio = s.expected / s.actual;
    if (ratio < 0.5 || ratio > 1.5) continue;
    all.push(ratio);
    const list = perFont.get(s.font);
    if (list === undefined) perFont.set(s.font, [ratio]);
    else list.push(ratio);
  }
  const byFont = new Map<string, number>();
  if (all.length < MIN_SAMPLES) return { byFont, global: null };
  const overall = median(all);
  const global =
    overall !== null && Math.abs(1 - overall) >= NEGLIGIBLE
      ? clampCorrection(overall)
      : null;
  for (const [font, ratios] of perFont) {
    if (ratios.length < MIN_FONT_SAMPLES) continue;
    const m = median(ratios);
    if (m !== null && Math.abs(1 - m) >= NEGLIGIBLE)
      byFont.set(font, clampCorrection(m));
  }
  return { byFont, global };
}

/** Multiply the `--scale-x` a span already has by `factor`. */
export function scaledX(current: string, factor: number): string {
  const value = Number.parseFloat(current);
  return String((Number.isFinite(value) ? value : 1) * factor);
}

/** Measure and correct the runs of one rendered text layer. */
export function calibrateSpans(
  spans: readonly HTMLElement[],
  widths: readonly (number | undefined)[],
  scale: number,
): void {
  const samples: WidthSample[] = [];
  const pairs: { span: HTMLElement; font: string }[] = [];
  spans.forEach((span, i) => {
    if (!span.isConnected || (span.textContent ?? "").trim() === "") return;
    const font = span.style.fontFamily || "default";
    pairs.push({ span, font });
    if (samples.length >= 600) return;
    samples.push({
      font,
      expected: Math.abs((widths[i] ?? 0) * scale),
      actual: span.getBoundingClientRect().width,
    });
  });
  const fix = corrections(samples);
  if (fix.byFont.size === 0 && fix.global === null) return;
  for (const { span, font } of pairs) {
    const factor = fix.byFont.get(font) ?? fix.global;
    if (factor === null || factor === undefined) continue;
    span.style.setProperty(
      "--scale-x",
      scaledX(span.style.getPropertyValue("--scale-x") || "1", factor),
    );
  }
}
