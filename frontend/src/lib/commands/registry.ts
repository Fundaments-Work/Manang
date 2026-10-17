/**
 * Command registry — the single source of truth behind the Cmd+K palette
 * (DESIGN.md §7), the keyboard map, and the Library actions.
 *
 * Commands are data, not a switch statement, so the palette, the shortcuts and
 * the tests all read the same list. Prompts read as direct human actions
 * ("Verify citations"), never as engine jargon.
 */

import { desk } from "$lib/state/desk.svelte";
import { detectPlatform } from "$lib/util/keyboard";

export type CommandGroup = "AI Actions" | "Plugins & Commands";

export interface Command {
  readonly id: string;
  readonly title: string;
  readonly group: CommandGroup;
  /** Extra searchable terms that do not appear in the title. */
  readonly keywords?: string;
  /** Human-facing shortcut, e.g. "⌘⇧D". */
  readonly hint?: string;
  readonly run: () => void;
}

const platform = detectPlatform();

function build(): Command[] {
  return [
    {
      id: "verify-citations",
      title: "Verify citations against Crossref & OpenAlex",
      group: "AI Actions",
      keywords: "crossref openalex doi bibliography references check",
      run: () => ink(),
    },
    {
      id: "generate-slides",
      title: "Generate 6 Touying presentation slides from Section 2",
      group: "AI Actions",
      keywords: "touying beamer deck presentation polylux",
      run: () => openSlides(),
    },
    {
      id: "add-cetz-diagram",
      title: "Add commutative diagram in CeTZ",
      group: "AI Actions",
      keywords: "cetz tikz diagram commutative arrows category",
      run: () => desk.toggleCanvas(),
    },

    {
      id: "open-canvas",
      title: "Open Drawing Canvas",
      group: "Plugins & Commands",
      keywords: "draw excalidraw vector figure ink sketch",
      hint: "/draw",
      run: () => desk.toggleCanvas(),
    },
    {
      id: "toggle-split",
      title: "Toggle Split Screen",
      group: "Plugins & Commands",
      hint: "/split",
      run: () => desk.togglePane(),
    },
    {
      id: "export-pdf",
      title: "Export Publication PDF",
      group: "Plugins & Commands",
      keywords: "publish print download",
      hint: "/export",
      run: () => ink(),
    },
    {
      id: "open-shelf",
      title: "Open Instrument Shelf",
      group: "Plugins & Commands",
      keywords: "plugins wasm extensions sandbox permissions",
      hint: "/plugins",
      run: () => {
        desk.shelfOpen = true;
      },
    },
    {
      id: "open-library",
      title: "Open Library Shelf",
      group: "Plugins & Commands",
      keywords: "files documents figures browse project",
      hint: `${platform.mod}P`,
      run: () => {
        desk.libraryOpen = !desk.libraryOpen;
      },
    },
    {
      id: "open-slides",
      title: "Open Presentation Studio",
      group: "Plugins & Commands",
      keywords: "slides deck carousel present",
      run: () => openSlides(),
    },
    {
      id: "toggle-zen",
      title: "Enter Zen Mode",
      group: "Plugins & Commands",
      keywords: "focus distraction free fullscreen",
      hint: `${platform.mod}\\`,
      run: () => desk.toggleZen(),
    },
  ];
}

/** The agent-backed actions are stubs until the Rust agent runtime lands. */
function ink(): void {
  /* Phase 4 wiring point: dispatch to the MCP host. */
}

export const commands: Command[] = build();

export function commandsInGroup(group: CommandGroup): Command[] {
  return commands.filter((command) => command.group === group);
}

/** The string the fuzzy matcher scores against. */
export function searchText(command: Command): string {
  return `${command.title} ${command.keywords ?? ""}`;
}

function openSlides(): void {
  desk.slideMode = true;
}
