/**
 * Links and the outline of a PDF, from what pdf.js reports: which link
 * annotations go where, where a destination is on its page, and the outline
 * as a flat, indented list. Pure; the lookups pdf.js does in its worker are
 * passed in.
 */

/** A PDF page reference as pdf.js hands it over. */
export interface PageRef {
  readonly num: number;
  readonly gen: number;
}

/** A named destination or an explicit `[page, /XYZ, left, top, zoom]` array. */
export type Destination = string | readonly unknown[];

export type LinkTarget =
  | { readonly kind: "external"; readonly url: string }
  | { readonly kind: "internal"; readonly dest: Destination };

/** The fields of a pdf.js annotation that matter for links. */
export interface AnnotationLike {
  readonly subtype?: string;
  readonly rect?: readonly number[];
  readonly url?: string;
  readonly unsafeUrl?: string;
  readonly dest?: unknown;
}

/** Only web and mail addresses leave the app; nothing else is opened. */
export function isSafeExternal(url: string): boolean {
  try {
    const parsed = new URL(url);
    return ["http:", "https:", "mailto:"].includes(parsed.protocol);
  } catch {
    return false;
  }
}

/** Where a link annotation goes, or null for anything that is not a usable link. */
export function linkTarget(annotation: AnnotationLike): LinkTarget | null {
  if (annotation.subtype !== "Link") return null;
  const url = annotation.url ?? annotation.unsafeUrl;
  if (typeof url === "string" && url !== "") {
    return isSafeExternal(url) ? { kind: "external", url } : null;
  }
  const dest = annotation.dest;
  if (typeof dest === "string" && dest !== "") return { kind: "internal", dest };
  if (Array.isArray(dest) && dest.length > 0) return { kind: "internal", dest };
  return null;
}

/**
 * A link's box as fractions of the page (top-left origin), from the PDF
 * rectangle `[x1, y1, x2, y2]` (bottom-left origin) and the page's `view`
 * box `[x0, y0, x1, y1]`.
 */
export function linkBox(
  rect: readonly number[],
  view: readonly number[],
): { left: number; top: number; width: number; height: number } | null {
  const [x1, y1, x2, y2] = rect;
  const [vx0 = 0, vy0 = 0, vx1 = 0, vy1 = 0] = view;
  const w = vx1 - vx0;
  const h = vy1 - vy0;
  if (
    x1 === undefined ||
    y1 === undefined ||
    x2 === undefined ||
    y2 === undefined ||
    w <= 0 ||
    h <= 0
  )
    return null;
  const left = Math.min(x1, x2) - vx0;
  const right = Math.max(x1, x2) - vx0;
  const top = vy1 - Math.max(y1, y2);
  const bottom = vy1 - Math.min(y1, y2);
  if (right - left <= 0 || bottom - top <= 0) return null;
  return {
    left: left / w,
    top: top / h,
    width: (right - left) / w,
    height: (bottom - top) / h,
  };
}

/** A destination resolved to a place: 0-based page, points from the top-left. */
export interface DestPoint {
  readonly page: number;
  /** Null when the destination keeps the horizontal position. */
  readonly x: number | null;
  /** Null when the destination means the top of the page. */
  readonly y: number | null;
}

export interface DestLookup {
  /** The explicit destination behind a name, or null. */
  getDestination(name: string): Promise<readonly unknown[] | null>;
  /** 0-based index of a page reference. */
  getPageIndex(ref: PageRef): Promise<number>;
  /** The `view` box of a 0-based page (`[x0, y0, x1, y1]`, points). */
  view(page: number): readonly number[] | null;
}

function isRef(value: unknown): value is PageRef {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as PageRef).num === "number" &&
    typeof (value as PageRef).gen === "number"
  );
}

function num(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

/** The fit type of an explicit destination: `/XYZ` arrives as `{ name: "XYZ" }`. */
function fitName(value: unknown): string {
  if (typeof value === "object" && value !== null && "name" in value) {
    const name = (value as { name: unknown }).name;
    return typeof name === "string" ? name : "";
  }
  return typeof value === "string" ? value : "";
}

/** Resolve a destination to its page and point; null when it leads nowhere. */
export async function resolveDest(
  dest: Destination,
  lookup: DestLookup,
): Promise<DestPoint | null> {
  const explicit = typeof dest === "string" ? await lookup.getDestination(dest) : dest;
  if (explicit === null || explicit.length === 0) return null;
  const [target, fit, ...args] = explicit;
  let page: number;
  if (isRef(target)) page = await lookup.getPageIndex(target);
  else if (typeof target === "number" && Number.isInteger(target) && target >= 0)
    page = target;
  else return null;
  const view = lookup.view(page);
  const x0 = view?.[0] ?? 0;
  const y1 = view?.[3] ?? null;
  const fromTop = (y: number | null): number | null =>
    y === null || y1 === null ? null : Math.max(0, y1 - y);
  switch (fitName(fit)) {
    case "XYZ": {
      const left = num(args[0]);
      return { page, x: left === null ? null : left - x0, y: fromTop(num(args[1])) };
    }
    case "FitH":
    case "FitBH":
      return { page, x: null, y: fromTop(num(args[0])) };
    case "FitV":
    case "FitBV": {
      const left = num(args[0]);
      return { page, x: left === null ? null : left - x0, y: null };
    }
    case "FitR": {
      const left = num(args[0]);
      return {
        page,
        x: left === null ? null : left - x0,
        y: fromTop(num(args[3])),
      };
    }
    default:
      return { page, x: null, y: null };
  }
}

/** An outline node as pdf.js returns it from `getOutline()`. */
export interface OutlineNode {
  readonly title: string;
  readonly dest: Destination | null;
  readonly url: string | null;
  readonly items: readonly OutlineNode[];
}

export interface OutlineEntry {
  /** Stable within one outline: the path of indices, "0.2.1". */
  readonly id: string;
  readonly title: string;
  readonly depth: number;
  readonly dest: Destination | null;
  readonly url: string | null;
}

/** The outline as a flat list in reading order, each entry with its depth. */
export function flattenOutline(
  nodes: readonly OutlineNode[] | null,
  maxDepth = 6,
): OutlineEntry[] {
  const out: OutlineEntry[] = [];
  const walk = (list: readonly OutlineNode[], depth: number, prefix: string): void => {
    list.forEach((node, i) => {
      const id = prefix === "" ? String(i) : `${prefix}.${i}`;
      out.push({
        id,
        title: node.title.replace(/\s+/g, " ").trim() || "Untitled",
        depth,
        dest: node.dest,
        url: node.url,
      });
      if (depth + 1 < maxDepth) walk(node.items, depth + 1, id);
    });
  };
  walk(nodes ?? [], 0, "");
  return out;
}
