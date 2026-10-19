/**
 * Bundled user documentation for in-app offline help.
 *
 * Uses Vite's `import.meta.glob` with `?raw` and `eager: true` so the entire
 * user guide is bundled directly into the frontend at build time and works
 * completely offline with zero network requests.
 */

export interface HelpDoc {
  readonly slug: string;
  readonly title: string;
  readonly order: number;
  readonly raw: string;
}

export interface SearchMatch {
  readonly doc: HelpDoc;
  readonly snippet: string;
  readonly heading?: string | undefined;
}

const RAW_DOCS = import.meta.glob<string>("../../../docs/user/*.md", {
  query: "?raw",
  import: "default",
  eager: true,
});

const DOC_ORDER: Record<string, number> = {
  index: 0,
  install: 0.5,
  "getting-started": 1,
  writing: 2,
  "preview-and-pdf": 3,
  "figures-and-diagrams": 4,
  references: 5,
  "projects-and-files": 6,
  "crash-recovery": 7,
  "version-history": 8,
  trash: 9,
  templates: 10,
  "settings-and-shortcuts": 11,
  "menus-and-shortcuts": 12,
  plugins: 13,
  troubleshooting: 14,
};

function extractTitle(raw: string, slug: string): string {
  const match = /^#\s+(.+)$/m.exec(raw);
  if (match?.[1]) {
    return match[1].trim();
  }
  return slug
    .split("-")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

export function loadAllDocs(): HelpDoc[] {
  const docs: HelpDoc[] = [];

  for (const [path, content] of Object.entries(RAW_DOCS)) {
    const filename = path.split("/").pop() ?? "";
    const slug = filename.replace(/\.md$/, "");
    const title = extractTitle(content, slug);
    const order = DOC_ORDER[slug] ?? 99;
    docs.push({
      slug,
      title,
      order,
      raw: content,
    });
  }

  return docs.sort((a, b) => a.order - b.order || a.slug.localeCompare(b.slug));
}

export const ALL_DOCS: readonly HelpDoc[] = loadAllDocs();

export const DOCS_BY_SLUG: ReadonlyMap<string, HelpDoc> = new Map(
  ALL_DOCS.map((doc) => [doc.slug, doc]),
);

export function getDoc(slug: string): HelpDoc | undefined {
  return DOCS_BY_SLUG.get(slug);
}

/**
 * Searches user guide documents for a case-insensitive query string.
 * Matches against titles, headings, and paragraph text.
 */
export function searchDocs(query: string, docs: readonly HelpDoc[] = ALL_DOCS): SearchMatch[] {
  const needle = query.trim().toLowerCase();
  if (needle === "") return [];

  const results: SearchMatch[] = [];

  for (const doc of docs) {
    const rawLower = doc.raw.toLowerCase();
    const titleLower = doc.title.toLowerCase();

    if (titleLower.includes(needle)) {
      // Find first matching sentence or first paragraph
      const lines = doc.raw.split("\n").filter((l) => l.trim() !== "" && !l.startsWith("#"));
      const snippet = lines[0]?.slice(0, 160) ?? doc.title;
      results.push({ doc, snippet });
      continue;
    }

    const index = rawLower.indexOf(needle);
    if (index !== -1) {
      // Extract a 140-char window around the match
      const start = Math.max(0, index - 40);
      const end = Math.min(doc.raw.length, index + 100);
      const rawSnippet = doc.raw.slice(start, end).replace(/\n+/g, " ").trim();
      const prefix = start > 0 ? "... " : "";
      const suffix = end < doc.raw.length ? " ..." : "";
      results.push({
        doc,
        snippet: `${prefix}${rawSnippet}${suffix}`,
      });
    }
  }

  return results;
}
