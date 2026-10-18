/**
 * Widgets of the visual layer: images, formulas, rules, front matter and list
 * markers. A widget replaces source text in the view only; the document is
 * never touched, and the source reappears when the caret reaches the range
 * (see `reveal.ts`).
 */

import { WidgetType, type EditorView } from "@codemirror/view";

import type { MathLanguage, VisualHost } from "./host";

const MIME: Readonly<Record<string, string>> = {
  svg: "image/svg+xml",
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  webp: "image/webp",
};

/** The MIME type of an image path, or null when it is not one a view can show. */
export function imageMime(path: string): string | null {
  const ext = /\.([A-Za-z0-9]+)$/.exec(path)?.[1]?.toLowerCase();
  return ext === undefined ? null : (MIME[ext] ?? null);
}

function element(tag: string, className: string, text?: string): HTMLElement {
  const el = document.createElement(tag);
  el.className = className;
  if (text !== undefined) el.textContent = text;
  return el;
}

/** Move the caret into the source the widget stands for (which then shows). */
function reveal(view: EditorView, dom: HTMLElement, event: Event): void {
  event.preventDefault();
  view.dispatch({ selection: { anchor: view.posAtDOM(dom) }, scrollIntoView: true });
  view.focus();
}

/* -------------------------------------------------------------------------- */

const mathCache = new Map<string, Promise<string>>();
const MATH_LIMIT = 200;

/** The blob URL of a formula's SVG, cached by content. */
function mathUrl(
  host: VisualHost,
  language: MathLanguage,
  source: string,
  size: number,
): Promise<string> {
  const key = `${language}\n${size}\n${source}`;
  let found = mathCache.get(key);
  if (found === undefined) {
    found = host
      .renderMath(language, source, size)
      .then((svg) => URL.createObjectURL(new Blob([svg], { type: "image/svg+xml" })));
    found.catch(() => mathCache.delete(key));
    mathCache.set(key, found);
    if (mathCache.size > MATH_LIMIT) {
      const oldest = mathCache.keys().next().value;
      if (oldest !== undefined) {
        const gone = mathCache.get(oldest);
        mathCache.delete(oldest);
        void gone?.then((url) => URL.revokeObjectURL(url)).catch(() => undefined);
      }
    }
  }
  return found;
}

export class MathWidget extends WidgetType {
  constructor(
    private readonly host: VisualHost,
    readonly language: MathLanguage,
    readonly source: string,
    readonly display: boolean,
  ) {
    super();
  }

  override eq(other: MathWidget): boolean {
    return (
      other.host === this.host &&
      other.language === this.language &&
      other.source === this.source &&
      other.display === this.display
    );
  }

  override toDOM(view: EditorView): HTMLElement {
    const dom = element(
      "span",
      this.display ? "cm-vis-math cm-vis-math-display" : "cm-vis-math",
    );
    dom.title = this.source;
    dom.dataset["vis"] = "math";
    const size = Math.round(
      (Number.parseFloat(getComputedStyle(view.contentDOM).fontSize) || 14) *
        (this.display ? 1.25 : 1.1),
    );
    dom.textContent = this.source;
    dom.classList.add("cm-vis-pending");
    mathUrl(this.host, this.language, this.source, size)
      .then((url) => {
        const img = document.createElement("img");
        img.className = "cm-vis-math-img";
        img.alt = this.source;
        img.src = url;
        dom.replaceChildren(img);
        dom.classList.remove("cm-vis-pending");
        view.requestMeasure();
      })
      .catch(() => {
        // A formula that does not compile stays as its source, flagged.
        dom.classList.remove("cm-vis-pending");
        dom.classList.add("cm-vis-math-error");
      });
    dom.addEventListener("mousedown", (event) => reveal(view, dom, event));
    return dom;
  }

  override ignoreEvent(): boolean {
    return true;
  }
}

/* -------------------------------------------------------------------------- */

export class ImageWidget extends WidgetType {
  constructor(
    private readonly host: VisualHost,
    readonly src: string,
    readonly alt: string,
    readonly figure: boolean,
  ) {
    super();
  }

  override eq(other: ImageWidget): boolean {
    return (
      other.host === this.host &&
      other.src === this.src &&
      other.alt === this.alt &&
      other.figure === this.figure
    );
  }

  override toDOM(view: EditorView): HTMLElement {
    const dom = element(
      "span",
      this.figure ? "cm-vis-image cm-vis-image-figure" : "cm-vis-image",
    );
    dom.dataset["vis"] = "image";
    dom.dataset["src"] = this.src;
    const path = this.host.resolveAsset(this.src);
    const mime = path === null ? null : imageMime(path);
    const chip = (text: string): void => {
      dom.replaceChildren(element("span", "cm-vis-chip", text));
    };
    if (path === null || mime === null) {
      chip(this.alt === "" ? this.src : this.alt);
      return dom;
    }
    chip(`Loading ${this.src}`);
    this.host
      .readBytes(path)
      .then((bytes) => {
        if (dom.dataset["gone"] === "1") return;
        const url = URL.createObjectURL(new Blob([bytes.slice()], { type: mime }));
        dom.dataset["url"] = url;
        const img = document.createElement("img");
        img.className = "cm-vis-image-img";
        img.alt = this.alt;
        img.draggable = false;
        img.addEventListener("load", () => view.requestMeasure());
        img.src = url;
        dom.replaceChildren(img);
      })
      .catch(() => chip(`Missing ${this.src}`));
    // A figure's exported SVG opens its scene in the figure studio.
    dom.addEventListener("dblclick", (event) => {
      event.preventDefault();
      const scene = mime === "image/svg+xml" ? this.host.figureFor(path) : null;
      if (scene !== null) this.host.openFigure(scene);
      else reveal(view, dom, event);
    });
    return dom;
  }

  override destroy(dom: HTMLElement): void {
    dom.dataset["gone"] = "1";
    const url = dom.dataset["url"];
    if (url !== undefined) URL.revokeObjectURL(url);
  }

  override ignoreEvent(): boolean {
    return true;
  }
}

/* -------------------------------------------------------------------------- */

export class RuleWidget extends WidgetType {
  override eq(other: RuleWidget): boolean {
    return other instanceof RuleWidget;
  }

  override toDOM(view: EditorView): HTMLElement {
    const dom = element("span", "cm-vis-rule");
    dom.dataset["vis"] = "rule";
    dom.addEventListener("mousedown", (event) => reveal(view, dom, event));
    return dom;
  }

  override ignoreEvent(): boolean {
    return true;
  }
}

export class FrontmatterWidget extends WidgetType {
  constructor(readonly lines: number) {
    super();
  }

  override eq(other: FrontmatterWidget): boolean {
    return other.lines === this.lines;
  }

  override toDOM(view: EditorView): HTMLElement {
    const dom = element(
      "span",
      "cm-vis-frontmatter",
      `Front matter, ${this.lines} ${this.lines === 1 ? "line" : "lines"}`,
    );
    dom.dataset["vis"] = "frontmatter";
    dom.addEventListener("mousedown", (event) => reveal(view, dom, event));
    return dom;
  }

  override ignoreEvent(): boolean {
    return true;
  }
}

/** A bullet, or a checkbox for a task item, in place of the list marker. */
export class ListMarkerWidget extends WidgetType {
  constructor(readonly task: "open" | "done" | null) {
    super();
  }

  override eq(other: ListMarkerWidget): boolean {
    return other.task === this.task;
  }

  override toDOM(): HTMLElement {
    if (this.task === null) {
      const dom = element("span", "cm-vis-bullet", "•");
      dom.dataset["vis"] = "bullet";
      return dom;
    }
    const dom = element(
      "span",
      this.task === "done" ? "cm-vis-check cm-vis-check-done" : "cm-vis-check",
    );
    dom.dataset["vis"] = "task";
    return dom;
  }
}
