import { describe, expect, it, vi } from "vitest";

import { flatten, groupRanked, paletteCandidates, rankCandidates } from "./palette";
import type { Command } from "./registry";
import { formatShortcut, matches } from "./shortcut";

const noop = (): void => undefined;
const workbench: Command[] = [
  { id: "view.toggle-panel", title: "Toggle bottom panel", group: "Workbench", shortcuts: [{ key: "j", mod: true }], run: noop },
  { id: "file.new", title: "New file", group: "Workbench", run: noop },
];
const kernel = [
  { id: "typst.export-pdf", title: "Export PDF", plugin: "typst" },
  { id: "editor.new", title: "New document", plugin: "editor" },
];
const base = {
  workbench,
  files: ["main.typ", "sections/results.typ"],
  kernel,
  runKernel: noop,
  openFile: noop,
};

describe("palette candidates", () => {
  it("commands mode lists workbench and plugin commands, not files, until you type", () => {
    const all = paletteCandidates({ ...base, mode: "commands", query: "" });
    expect(all.map((c) => c.group)).toEqual([
      "Workbench",
      "Workbench",
      "Plugin commands",
      "Plugin commands",
    ]);
    const typed = paletteCandidates({ ...base, mode: "commands", query: "res" });
    expect(typed.some((c) => c.id === "file:sections/results.typ")).toBe(true);
  });

  it("files mode lists only files", () => {
    const all = paletteCandidates({ ...base, mode: "files", query: "" });
    expect(all.map((c) => c.id)).toEqual(["file:main.typ", "file:sections/results.typ"]);
    expect(all[1]).toMatchObject({ title: "results.typ", hint: "sections" });
  });

  it("opens the chosen file", () => {
    const open = vi.fn();
    const [, second] = paletteCandidates({ ...base, openFile: open, mode: "files", query: "" });
    second?.run();
    expect(open).toHaveBeenCalledWith("sections/results.typ");
  });

  it("runs kernel commands by their real id", () => {
    const run = vi.fn();
    const cmd = paletteCandidates({ ...base, runKernel: run, mode: "commands", query: "" }).find(
      (c) => c.id === "kernel:typst.export-pdf",
    );
    cmd?.run();
    expect(run).toHaveBeenCalledWith("typst.export-pdf");
  });
});

describe("grouping", () => {
  it("walks the cursor in display order, not rank order", () => {
    const candidates = paletteCandidates({ ...base, mode: "commands", query: "n" });
    const groups = groupRanked(rankCandidates(candidates, "n"));
    const flat = flatten(groups);
    expect(flat.map((c) => c.group)).toEqual(
      [...flat.map((c) => c.group)].sort(
        (a, b) => ["Workbench", "Files", "Plugin commands"].indexOf(a) - ["Workbench", "Files", "Plugin commands"].indexOf(b),
      ),
    );
    expect(groups.every((g) => g.entries.length > 0)).toBe(true);
  });

  it("finds a command by its plugin and raw id", () => {
    const ranked = rankCandidates(
      paletteCandidates({ ...base, mode: "commands", query: "typst" }),
      "typst",
    );
    expect(ranked[0]?.item.id).toBe("kernel:typst.export-pdf");
  });
});

describe("shortcuts", () => {
  const key = (over: Partial<{ key: string; ctrlKey: boolean; metaKey: boolean; shiftKey: boolean; altKey: boolean }>) => ({
    key: "k",
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    altKey: false,
    ...over,
  });

  it("matches Ctrl or Cmd, and is exact about Shift and Alt", () => {
    const s = { key: "p", mod: true, shift: true };
    expect(matches(key({ key: "P", ctrlKey: true, shiftKey: true }), s)).toBe(true);
    expect(matches(key({ key: "p", metaKey: true, shiftKey: true }), s)).toBe(true);
    expect(matches(key({ key: "p", ctrlKey: true }), s)).toBe(false);
    expect(matches(key({ key: "p", ctrlKey: true, shiftKey: true, altKey: true }), s)).toBe(false);
    expect(matches(key({ key: "p", shiftKey: true }), s)).toBe(false);
  });

  it("formats for the platform", () => {
    expect(formatShortcut({ key: "p", mod: true, shift: true }, { isMac: false })).toBe("Ctrl+Shift+P");
    expect(formatShortcut({ key: "p", mod: true, shift: true }, { isMac: true })).toBe("⇧⌘P");
    expect(formatShortcut({ key: "PageDown", mod: true }, { isMac: false })).toBe("Ctrl+PgDn");
    expect(formatShortcut({ key: "\\", mod: true }, { isMac: false })).toBe("Ctrl+\\");
  });
});
