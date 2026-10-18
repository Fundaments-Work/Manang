import { describe, expect, it } from "vitest";

import type { KernelCommand } from "$lib/kernel/client";
import { createDevKernel } from "$lib/kernel/dev";

import {
  isPaletteSafe,
  paletteAction,
  paletteKernelCommands,
  PALETTE_ALLOWLIST,
} from "./allowlist";
import { paletteCandidates } from "./palette";
import type { Command } from "./registry";

const noop = (): void => undefined;

const cmd = (id: string): KernelCommand => ({
  id,
  title: id,
  plugin: id.split(".")[0] ?? id,
});

describe("palette allowlist", () => {
  it("hides internals and commands that need arguments", () => {
    const hidden = [
      "editor.new",
      "editor.insert",
      "editor.text",
      "project.open",
      "project.rename",
      "project.delete",
      "typst.compile",
      "typst.render",
      "search.cancel",
      "plugins.register",
      "settings.set",
      "settings.get",
      "writing.spell.add-word",
      "writing.spell.suggest",
      "writing.snippets.expand",
    ];
    for (const id of hidden) expect(isPaletteSafe(id)).toBe(false);
    expect(paletteKernelCommands(hidden.map(cmd))).toEqual([]);
  });

  it("keeps the safe commands, in the kernel's order", () => {
    const list = [
      "editor.new",
      "typst.export-pdf",
      "writing.spell.check",
      "writing.stats.count",
    ];
    expect(paletteKernelCommands(list.map(cmd)).map((c) => c.id)).toEqual([
      "typst.export-pdf",
      "writing.spell.check",
      "writing.stats.count",
    ]);
  });

  it("does not repeat a command a workbench command already offers", () => {
    const list = ["typst.export-pdf", "writing.format.document"].map(cmd);
    expect(paletteKernelCommands(list, new Set())).toHaveLength(2);
    expect(
      paletteKernelCommands(list, new Set(["doc.export-pdf", "doc.format"])),
    ).toEqual([]);
  });

  it("maps editor-bound commands to shell actions, and plain ones to a kernel call", () => {
    expect(paletteAction("writing.format.document")).toBe("format");
    expect(paletteAction("writing.spell.check")).toBe("spell");
    expect(paletteAction("writing.stats.count")).toBe("stats");
    expect(paletteAction("typst.export-pdf")).toBe("export-pdf");
    expect(paletteAction("figure.create")).toBe("new-figure");
    expect(paletteAction("editor.new")).toBeNull();
  });

  it("only ever lists ids that exist on the dev kernel", async () => {
    const ids = new Set((await createDevKernel().commands()).map((c) => c.id));
    for (const id of Object.keys(PALETTE_ALLOWLIST)) expect(ids.has(id)).toBe(true);
  });

  it("is not fooled by inherited property names", () => {
    expect(isPaletteSafe("toString")).toBe(false);
    expect(isPaletteSafe("__proto__")).toBe(false);
    expect(paletteAction("constructor")).toBeNull();
  });

  it("the palette lists only allowlisted plugin commands", async () => {
    const workbench: Command[] = [
      { id: "doc.export-pdf", title: "Export PDF", group: "Workbench", run: noop },
      // Covers `figure.create`, so the plugin entry is not listed twice.
      { id: "file.new-figure", title: "New figure", group: "Workbench", run: noop },
    ];
    const kernel = await createDevKernel().commands();
    const all = paletteCandidates({
      mode: "commands",
      query: "",
      workbench,
      files: [],
      kernel,
      runKernel: noop,
      openFile: noop,
    });
    const plugin = all.filter((c) => c.group === "Plugin commands").map((c) => c.hint);
    expect(plugin).toEqual([
      "writing.format.document",
      "writing.spell.check",
      "writing.stats.count",
    ]);
  });
});
