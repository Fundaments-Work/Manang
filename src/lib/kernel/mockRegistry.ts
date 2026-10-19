/**
 * A small sample plugin registry for the browser kernel (`pnpm dev`, tests):
 * what `index.json` would list, in the shape the Rust marketplace
 * (`crates/manang-plugins/src/marketplace.rs`) answers `plugins.browse` and
 * `plugins.detail` with. Five plugins cover the states the Plugins view shows:
 * an update that asks for nothing new (`word-count`), an update that asks for
 * more access (`slide-themes`), one already waiting for approval
 * (`plot-export`), two not installed (`table-tools`, `crossref-lookup`, the
 * second with a withdrawn version).
 */

import type {
  BrowseEntry,
  PluginDetail,
  RegistryAccess,
  RegistryVersion,
} from "./marketApi";

export interface MockRegistryPlugin {
  readonly id: string;
  readonly name: string;
  readonly author: string;
  readonly description: string;
  readonly repo: string;
  readonly homepage?: string;
  readonly license: string;
  readonly tags: readonly string[];
  readonly readme: string;
  readonly downloads: number;
  /** Oldest first, as the registry lists them. */
  readonly versions: readonly RegistryVersion[];
  readonly latest: string;
}

const none: RegistryAccess = { filesystem: [], network: [], buffer: [] };

const version = (
  v: string,
  published: string,
  size: number,
  permissions: Partial<RegistryAccess>,
  changelog: string,
  extra: Partial<RegistryVersion> = {},
): RegistryVersion => ({
  version: v,
  published,
  size,
  min_app: null,
  changelog,
  permissions: { ...none, ...permissions },
  compatible: true,
  yanked: false,
  ...extra,
});

export const MOCK_REGISTRY: readonly MockRegistryPlugin[] = [
  {
    id: "word-count",
    name: "Word count",
    author: "Fundaments.work",
    description: "Counts words, characters and lines in the open document.",
    repo: "https://github.com/Fundaments-Work/manang-word-count",
    license: "MIT",
    tags: ["writing", "statistics"],
    downloads: 4210,
    readme:
      "# Word count\n\nCounts **words**, characters and lines in the open document and shows them in the status bar.\n\n## Use\n\n- Run *Count words* from the command palette.\n- Select text to count only the selection.\n\nIt only reads the document you have open.\n",
    latest: "0.2.0",
    versions: [
      version(
        "0.1.0",
        "2026-08-02T09:00:00Z",
        41_200,
        { buffer: ["read:active"] },
        "First release.",
      ),
      version(
        "0.2.0",
        "2026-10-10T12:00:00Z",
        48_211,
        { buffer: ["read:active"] },
        "Fixes counting of CJK text and adds a selection count.",
      ),
    ],
  },
  {
    id: "slide-themes",
    name: "Slide themes",
    author: "Example Labs",
    description: "Extra themes for slide decks, with optional web fonts.",
    repo: "https://github.com/example-labs/manang-slide-themes",
    homepage: "https://example.org/slide-themes",
    license: "Apache-2.0",
    tags: ["slides", "design"],
    downloads: 980,
    readme:
      "# Slide themes\n\nThree calm themes for Typst slide decks.\n\nVersion 0.2.0 can fetch web fonts from `fonts.example.org` and asks for network access to do so.\n",
    latest: "0.2.0",
    versions: [
      version("0.1.0", "2026-06-20T10:00:00Z", 30_100, {}, "First release."),
      version(
        "0.2.0",
        "2026-10-15T08:30:00Z",
        36_400,
        { network: ["fonts.example.org"] },
        "Adds web fonts. Needs network access to fonts.example.org.",
      ),
    ],
  },
  {
    id: "plot-export",
    name: "Plot export",
    author: "Example Labs",
    description: "Exports figures as publication-ready plots.",
    repo: "https://github.com/example-labs/manang-plot-export",
    license: "MIT",
    tags: ["figures", "export"],
    downloads: 2305,
    readme:
      "# Plot export\n\nTurns a figure into a plot with axes, ticks and a legend.\n",
    latest: "0.3.0",
    versions: [
      version(
        "0.2.0",
        "2026-07-11T10:00:00Z",
        52_000,
        { filesystem: ["write:project"] },
        "Adds SVG output.",
      ),
      version(
        "0.3.0",
        "2026-10-01T10:00:00Z",
        55_800,
        { filesystem: ["write:project"], network: ["plots.example.org"] },
        "Uploads a preview to plots.example.org when asked.",
      ),
    ],
  },
  {
    id: "table-tools",
    name: "Table tools",
    author: "A. Author",
    description: "Align, sort and convert Markdown and Typst tables.",
    repo: "https://github.com/a-author/manang-table-tools",
    license: "MIT",
    tags: ["writing", "tables"],
    downloads: 12_480,
    readme:
      "# Table tools\n\nSelect a table and run *Align table*.\n\n| Command | Does |\n| --- | --- |\n| Align table | Pads the columns |\n| Sort by column | Sorts the rows |\n",
    latest: "1.0.0",
    versions: [
      version(
        "0.9.0",
        "2026-05-04T10:00:00Z",
        24_000,
        { buffer: ["write:active"] },
        "Beta.",
      ),
      version(
        "1.0.0",
        "2026-09-12T10:00:00Z",
        26_900,
        { buffer: ["write:active"] },
        "First stable release.",
      ),
    ],
  },
  {
    id: "crossref-lookup",
    name: "Crossref lookup",
    author: "Open Refs",
    description: "Look up a DOI on Crossref and add the entry to your bibliography.",
    repo: "https://github.com/open-refs/manang-crossref",
    license: "BSD-3-Clause",
    tags: ["references", "citations"],
    downloads: 730,
    readme:
      "# Crossref lookup\n\nPaste a DOI; the entry is added to the project's `.bib` file.\n",
    latest: "0.5.0",
    versions: [
      version(
        "0.4.1",
        "2026-07-30T10:00:00Z",
        33_000,
        { network: ["api.crossref.org"], filesystem: ["read:project"] },
        "Sends your project path by mistake.",
        { yanked: true },
      ),
      version(
        "0.5.0",
        "2026-09-30T10:00:00Z",
        33_500,
        {
          network: ["api.crossref.org"],
          filesystem: ["read:project", "write:project"],
        },
        "Fixes the withdrawn 0.4.1 and writes entries to the .bib file.",
        { min_app: "0.1.0" },
      ),
    ],
  },
];

export function latestOf(p: MockRegistryPlugin): RegistryVersion | undefined {
  return p.versions.find((v) => v.version === p.latest);
}

export function toBrowseEntry(
  p: MockRegistryPlugin,
  installedVersion: string | null,
  isNewer: (latest: string, installed: string) => boolean,
): BrowseEntry {
  const latest = latestOf(p);
  return {
    id: p.id,
    name: p.name,
    author: p.author,
    description: p.description,
    tags: p.tags,
    license: p.license,
    latest: p.latest,
    published: latest?.published ?? null,
    size: latest?.size ?? null,
    downloads: p.downloads,
    compatible: latest?.compatible ?? true,
    installed:
      installedVersion === null
        ? null
        : {
            version: installedVersion,
            update_available: isNewer(p.latest, installedVersion),
          },
  };
}

export function toDetail(
  p: MockRegistryPlugin,
  entry: BrowseEntry,
  freshness: PluginDetail["freshness"],
): PluginDetail {
  return {
    freshness,
    plugin: entry,
    repo: p.repo,
    homepage: p.homepage ?? null,
    tier: "wasm",
    permissions: latestOf(p)?.permissions ?? null,
    versions: p.versions,
    readme: p.readme,
    readme_error: null,
  };
}
