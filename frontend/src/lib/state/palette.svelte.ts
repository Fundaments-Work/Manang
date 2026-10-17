/**
 * Command palette state — DESIGN.md §7.
 *
 * Keyboard-first: ↑ and ↓ to move, Enter to execute, Esc to dismiss. The
 * selected row is derived from the query rather than stored independently, so
 * typing can never leave the highlight pointing at a row that has been filtered
 * out from under it.
 */

import {
  commands,
  searchText,
  type Command,
  type CommandGroup,
} from "$lib/commands/registry";
import { fuzzyRank, type Ranked } from "$lib/commands/fuzzy";

const GROUP_ORDER: readonly CommandGroup[] = ["AI Actions", "Plugins & Commands"];

export class PaletteState {
  open = $state(false);

  query = $state("");

  /** Index into the flattened result list. */
  cursor = $state(0);

  readonly ranked: Ranked<Command>[] = $derived(
    fuzzyRank(commands, this.query, searchText),
  );

  readonly selected: Command | undefined = $derived(this.ranked[this.cursor]?.item);

  /** Results grouped for display, preserving the palette's section order. */
  readonly groups: { group: CommandGroup; entries: Ranked<Command>[] }[] = $derived.by(
    () => {
      const buckets: { group: CommandGroup; entries: Ranked<Command>[] }[] =
        GROUP_ORDER.map((group) => ({ group, entries: [] }));

      for (const entry of this.ranked) {
        const bucket = buckets.find(
          (candidate) => candidate.group === entry.item.group,
        );
        if (bucket !== undefined) bucket.entries.push(entry);
      }

      return buckets.filter((bucket) => bucket.entries.length > 0);
    },
  );

  /**
   * Flat row index of each result, so rendering a highlighted row is a lookup
   * rather than a second counting pass in the template.
   */
  readonly flatOrder: Command[] = $derived(this.ranked.map((entry) => entry.item));

  show(): void {
    this.open = true;
    this.query = "";
    this.cursor = 0;
  }

  /** Opens the palette with a query already typed, e.g. from a deep link. */
  showWith(query: string): void {
    this.show();
    this.setQuery(query);
  }

  hide(): void {
    this.open = false;
    this.query = "";
    this.cursor = 0;
  }

  toggle(): void {
    if (this.open) this.hide();
    else this.show();
  }

  setQuery(next: string): void {
    this.query = next;
    this.cursor = 0;
  }

  /** Moves the cursor, clamping at both ends. */
  move(delta: number): void {
    const max = this.flatOrder.length - 1;
    if (max < 0) return;
    this.cursor = Math.max(0, Math.min(this.cursor + delta, max));
  }

  /** Runs the selected command and closes. */
  execute(): void {
    const command = this.selected;
    if (command === undefined) return;
    this.hide();
    command.run();
  }
}

export const palette = new PaletteState();
