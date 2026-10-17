import { describe, expect, it, vi } from "vitest";

import { fuzzyRank } from "./fuzzy";
import { mergeCommands } from "./kernel";
import type { Command } from "./registry";

const noop = (): void => undefined;

const statics: Command[] = [
  {
    id: "toggle-split",
    title: "Toggle Split Screen",
    group: "Plugins & Commands",
    run: noop,
  },
  { id: "verify", title: "Verify citations", group: "AI Actions", run: noop },
];

const kernelList = [
  { id: "editor.new", title: "New document", plugin: "editor" },
  { id: "typst.render", title: "Render with Typst", plugin: "typst" },
  { id: "editor.new", title: "New document", plugin: "editor" },
];

describe("mergeCommands", () => {
  it("appends kernel commands after the static ones, in group Plugins & Commands", () => {
    const merged = mergeCommands(statics, kernelList, noop);

    expect(merged.map((c) => c.id)).toEqual([
      "toggle-split",
      "verify",
      "kernel:editor.new",
      "kernel:typst.render",
    ]);
    expect(merged.slice(2).every((c) => c.group === "Plugins & Commands")).toBe(true);
  });

  it("does not mutate the static list and tolerates an empty kernel", () => {
    expect(mergeCommands(statics, [], noop)).toEqual(statics);
    expect(statics).toHaveLength(2);
  });

  it("runs the kernel command id, not the prefixed palette id", () => {
    const run = vi.fn();
    const entry = mergeCommands([], kernelList, run)[1];

    entry?.run();

    expect(run).toHaveBeenCalledWith("typst.render");
  });

  it("is searchable by title, plugin and raw id through the existing fuzzy ranker", () => {
    const merged = mergeCommands(statics, kernelList, noop);
    const text = (c: Command): string => `${c.title} ${c.keywords ?? ""}`;

    expect(fuzzyRank(merged, "render", text)[0]?.item.id).toBe("kernel:typst.render");
    expect(fuzzyRank(merged, "typst.render", text)[0]?.item.id).toBe(
      "kernel:typst.render",
    );
  });
});
