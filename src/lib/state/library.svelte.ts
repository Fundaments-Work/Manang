/**
 * Library shelf contents — DESIGN.md §5.
 *
 * A local-first app is a folder on disk. This store mirrors that folder so the
 * rail, the palette and the figure references all agree on one tree.
 */

import { fuzzyRank } from "$lib/commands/fuzzy";

export type EntryKind = "document" | "figure" | "bibliography" | "folder";

export interface LibraryEntry {
  readonly id: string;
  readonly name: string;
  readonly kind: EntryKind;
  /** Relative to the project root. */
  readonly path: string;
  /** For documents. */
  readonly language?: "typst" | "latex" | "markdown";
  /** For bibliography: number of parsed entries. */
  readonly itemCount?: number;
}

export interface LibraryFolder {
  readonly id: string;
  readonly name: string;
  readonly entries: readonly LibraryEntry[];
}

const SEED: readonly LibraryFolder[] = [
  {
    id: "papers",
    name: "papers",
    entries: [
      {
        id: "d-formalism",
        name: "01_formalism.typ",
        kind: "document",
        path: "papers/01_formalism.typ",
        language: "typst",
      },
      {
        id: "d-entropy",
        name: "02_entropy_bounds.typ",
        kind: "document",
        path: "papers/02_entropy_bounds.typ",
        language: "typst",
      },
      {
        id: "d-numerical",
        name: "03_numerical_sim.typ",
        kind: "document",
        path: "papers/03_numerical_sim.typ",
        language: "typst",
      },
    ],
  },
  {
    id: "figures",
    name: "figures",
    entries: [
      {
        id: "f-lattice",
        name: "state_lattice.svg",
        kind: "figure",
        path: "figures/state_lattice.svg",
      },
      {
        id: "f-dissipation",
        name: "dissipation_curve.svg",
        kind: "figure",
        path: "figures/dissipation_curve.svg",
      },
    ],
  },
  {
    id: "bibliography",
    name: "bibliography",
    entries: [
      {
        id: "b-references",
        name: "references.bib",
        kind: "bibliography",
        path: "references.bib",
        itemCount: 3,
      },
    ],
  },
];

export class LibraryState {
  readonly folders: readonly LibraryFolder[] = $state(structuredClone(SEED));

  query = $state("");

  activeId = $state("d-entropy");

  /**
   * Every entry, flattened, with its folder name.
   *
   * Search runs across the flattened list so typing "svg" or "entropy" both work
   * regardless of which folder holds the match.
   */
  readonly allEntries: { folder: LibraryFolder; entry: LibraryEntry }[] = $derived(
    this.folders.flatMap((folder) =>
      folder.entries.map((entry) => ({ folder, entry })),
    ),
  );

  readonly filtered: { folder: LibraryFolder; entry: LibraryEntry }[] = $derived.by(
    () => {
      const needle = this.query.trim();
      if (needle.length === 0) return this.allEntries;

      // Fuzzy, like the command palette: typing "svgn" should still find
      // `state_lattice.svg`, because that is what the hand is doing.
      return fuzzyRank(
        this.allEntries,
        needle,
        ({ folder, entry }) => `${folder.name} ${entry.name}`,
      ).map((ranked) => ranked.item);
    },
  );

  /** Folders that still contain at least one match, preserving order. */
  readonly visibleFolders: { folder: LibraryFolder; entries: LibraryEntry[] }[] =
    $derived.by(() => {
      const grouped: { folder: LibraryFolder; entries: LibraryEntry[] }[] = [];
      for (const folder of this.folders) {
        const entries = this.filtered
          .filter((row) => row.folder.id === folder.id)
          .map((row) => row.entry);
        if (entries.length > 0) grouped.push({ folder, entries });
      }
      return grouped;
    });

  /** Paths of every figure, used to resolve `#image(…)` references. */
  get figurePaths(): string[] {
    return this.allEntries
      .filter(({ entry }) => entry.kind === "figure")
      .map(({ entry }) => entry.path);
  }

  setQuery(next: string): void {
    this.query = next;
  }

  activate(id: string): void {
    this.activeId = id;
  }

  isActive(id: string): boolean {
    return this.activeId === id;
  }
}

export const library = new LibraryState();
