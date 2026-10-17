/**
 * Slides are a view of a Typst document, not a separate file type: every
 * heading starts a slide, and the paragraphs and display equations under it
 * are its content. The shell shows a thumbnail strip and a present mode from
 * this parse; the rendered page itself still comes from the engine.
 */

export interface Slide {
  readonly id: string;
  /** 0-based position in the deck. */
  readonly index: number;
  readonly title: string;
  /** Heading depth: 1 is a title slide, 2 and deeper are content slides. */
  readonly level: number;
  readonly paragraphs: readonly string[];
  /** The first display equation, if the slide has one. */
  readonly math: string | null;
}

function emptySlide(
  index: number,
  title: string,
  level: number,
): {
  title: string;
  level: number;
  index: number;
  paragraphs: string[];
  math: string | null;
} {
  return { index, title, level, paragraphs: [], math: null };
}

export function parseSlides(text: string): Slide[] {
  const slides: ReturnType<typeof emptySlide>[] = [];
  let paragraph: string[] = [];
  let current: ReturnType<typeof emptySlide> | undefined;

  const flush = (): void => {
    if (current !== undefined && paragraph.length > 0)
      current.paragraphs.push(paragraph.join(" "));
    paragraph = [];
  };

  for (const raw of text.split("\n")) {
    const line = raw.trim();
    const heading = /^(=+)\s+(.+)$/.exec(line);
    if (heading !== null) {
      flush();
      current = emptySlide(slides.length, heading[2] ?? "", (heading[1] ?? "=").length);
      slides.push(current);
      continue;
    }
    if (current === undefined) continue;
    const display = /^\$\s+(.+?)\s+\$$/.exec(line);
    if (display !== null) {
      flush();
      current.math ??= display[1] ?? null;
    } else if (line === "" || line.startsWith("#")) {
      flush();
    } else {
      paragraph.push(line);
    }
  }
  flush();
  return slides.map((slide) => ({ ...slide, id: `slide-${slide.index}` }));
}

export function clampSlide(index: number, count: number): number {
  return count === 0 ? 0 : Math.max(0, Math.min(index, count - 1));
}
