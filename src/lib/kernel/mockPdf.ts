/**
 * A small PDF writer for the browser dev kernel: real, valid PDF files with
 * selectable text (the standard Helvetica and Courier fonts), an outline,
 * internal links (`/Dest`) and web links (`/URI`). Lets the PDF viewer be
 * exercised in the browser harness without a Typst or TeX engine.
 *
 * Coordinates are given the way the app speaks them, in points from the top
 * left of the page; the writer flips them to PDF space. ASCII text only: other
 * characters are written as `?`.
 */

export type MockFont = "sans" | "mono";

export interface MockText {
  readonly x: number;
  /** Baseline, from the top of the page. */
  readonly y: number;
  readonly size: number;
  readonly text: string;
  readonly font?: MockFont;
  /** Grey (0 black, 1 white). */
  readonly grey?: number;
}

export interface MockRect {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

export interface MockLink {
  /** The clickable box, top-left origin. */
  readonly rect: MockRect;
  readonly to: { readonly url: string } | { readonly page: number; readonly y: number };
}

export interface MockPage {
  readonly texts: readonly MockText[];
  readonly rects?: readonly MockRect[];
  readonly links?: readonly MockLink[];
}

export interface MockOutlineEntry {
  readonly title: string;
  readonly page: number;
  /** Points from the top of the page. */
  readonly y: number;
}

export interface MockPdf {
  readonly width: number;
  readonly height: number;
  /** Fill of the page, as RGB in 0..1; omitted leaves it white. */
  readonly paper?: readonly [number, number, number];
  readonly pages: readonly MockPage[];
  readonly outline?: readonly MockOutlineEntry[];
}

/** Helvetica advance widths (1/1000 em), WinAnsi codes 32..126. */
const HELVETICA = [
  278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556,
  556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667,
  667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722,
  667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500,
  556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, 556, 556, 333, 500, 278,
  556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];

/** Printable ASCII; anything else becomes `?` (as the writer draws it). */
export function ascii(text: string): string {
  return text.replace(/[^\x20-\x7e]/g, "?");
}

/** Width of `text` in points as the PDF lays it out. */
export function textWidth(text: string, size: number, font: MockFont = "sans"): number {
  let units = 0;
  for (const ch of ascii(text)) {
    units += font === "mono" ? 600 : (HELVETICA[ch.charCodeAt(0) - 32] ?? 556);
  }
  return (units * size) / 1000;
}

function str(text: string): string {
  return `(${ascii(text).replace(/([\\()])/g, "\\$1")})`;
}

function n(value: number): string {
  return String(Math.round(value * 1000) / 1000);
}

/** Write the document. Deterministic: the same input gives the same bytes. */
export function writePdf(doc: MockPdf): Uint8Array {
  const H = doc.height;
  const objects: string[] = [];
  const add = (body: string): number => {
    objects.push(body);
    return objects.length;
  };
  const reserve = (): number => add("");
  const set = (id: number, body: string): void => {
    objects[id - 1] = body;
  };

  const catalog = reserve();
  const pagesId = reserve();
  const sans = add(
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
  );
  const mono = add(
    "<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>",
  );
  const pageIds = doc.pages.map(() => reserve());
  const dest = (page: number, y: number): string =>
    `[${pageIds[Math.max(0, Math.min(page, pageIds.length - 1))] ?? 0} 0 R /XYZ 0 ${n(H - y)} null]`;

  doc.pages.forEach((page, index) => {
    const ops: string[] = [];
    if (doc.paper !== undefined) {
      const [r, g, b] = doc.paper;
      ops.push(`${n(r)} ${n(g)} ${n(b)} rg 0 0 ${n(doc.width)} ${n(H)} re f`);
    }
    for (const rect of page.rects ?? []) {
      ops.push(
        `0.42 G 0.5 w ${n(rect.x)} ${n(H - rect.y - rect.height)} ${n(rect.width)} ${n(rect.height)} re S`,
      );
    }
    for (const t of page.texts) {
      const font = t.font === "mono" ? "/F2" : "/F1";
      ops.push(
        `BT ${n(t.grey ?? 0.11)} g ${font} ${n(t.size)} Tf ${n(t.x)} ${n(H - t.y)} Td ${str(t.text)} Tj ET`,
      );
    }
    const stream = ops.join("\n");
    const content = add(`<< /Length ${stream.length} >>\nstream\n${stream}\nendstream`);
    const annots = (page.links ?? []).map((link) => {
      const { x, y, width, height } = link.rect;
      const rect = `[${n(x)} ${n(H - y - height)} ${n(x + width)} ${n(H - y)}]`;
      const action =
        "url" in link.to
          ? `/A << /S /URI /URI ${str(link.to.url)} >>`
          : `/Dest ${dest(link.to.page, link.to.y)}`;
      return add(
        `<< /Type /Annot /Subtype /Link /Rect ${rect} /Border [0 0 0] ${action} >>`,
      );
    });
    set(
      pageIds[index] ?? 0,
      `<< /Type /Page /Parent ${pagesId} 0 R /MediaBox [0 0 ${n(doc.width)} ${n(H)}] /Resources << /Font << /F1 ${sans} 0 R /F2 ${mono} 0 R >> >> /Contents ${content} 0 R${
        annots.length > 0 ? ` /Annots [${annots.map((a) => `${a} 0 R`).join(" ")}]` : ""
      } >>`,
    );
  });

  set(
    pagesId,
    `<< /Type /Pages /Kids [${pageIds.map((id) => `${id} 0 R`).join(" ")}] /Count ${pageIds.length} >>`,
  );

  const entries = doc.outline ?? [];
  let outlines = "";
  if (entries.length > 0) {
    const root = reserve();
    const ids = entries.map(() => reserve());
    entries.forEach((entry, i) => {
      const prev = i > 0 ? ` /Prev ${ids[i - 1]} 0 R` : "";
      const next = i < ids.length - 1 ? ` /Next ${ids[i + 1]} 0 R` : "";
      set(
        ids[i] ?? 0,
        `<< /Title ${str(entry.title)} /Parent ${root} 0 R${prev}${next} /Dest ${dest(entry.page, entry.y)} >>`,
      );
    });
    set(
      root,
      `<< /Type /Outlines /First ${ids[0]} 0 R /Last ${ids[ids.length - 1]} 0 R /Count ${ids.length} >>`,
    );
    outlines = ` /Outlines ${root} 0 R`;
  }
  set(catalog, `<< /Type /Catalog /Pages ${pagesId} 0 R${outlines} >>`);

  let body = "%PDF-1.4\n";
  const offsets: number[] = [];
  objects.forEach((object, i) => {
    offsets.push(body.length);
    body += `${i + 1} 0 obj\n${object}\nendobj\n`;
  });
  const xref = body.length;
  body += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
  for (const offset of offsets)
    body += `${String(offset).padStart(10, "0")} 00000 n \n`;
  body += `trailer\n<< /Size ${objects.length + 1} /Root ${catalog} 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  return new TextEncoder().encode(body);
}
