/**
 * Workbench commands: panels, tabs, files and documents.
 *
 * One list drives both the palette and the global keyboard handler, so a
 * shortcut shown beside a command is by construction the one that runs it.
 * Commands are rebuilt each time the palette opens so `enabled` is current.
 */

import { build } from "$lib/state/build.svelte";
import { documents } from "$lib/state/documents.svelte";
import { layout } from "$lib/state/layout.svelte";
import { tabs } from "$lib/state/tabs.svelte";
import { workspace } from "$lib/state/workspace.svelte";
import * as actions from "$lib/state/actions";
import { engineOf } from "$lib/workbench/paths";

import type { Command } from "./registry";
import type { Shortcut } from "./shortcut";

const mod = (key: string, extra: Partial<Shortcut> = {}): Shortcut => ({
  key,
  mod: true,
  ...extra,
});

/** Opening the palette has no command of its own: it is how commands are reached. */
export const PALETTE_COMMANDS_SHORTCUTS: readonly Shortcut[] = [
  mod("k"),
  mod("p", { shift: true }),
];
export const PALETTE_FILES_SHORTCUT: Shortcut = mod("p");

export function workbenchCommands(
  openPalette: (mode: "commands" | "files") => void,
): Command[] {
  const hasProject = workspace.project !== null;
  const active = tabs.active;
  const isDocument = active?.kind === "document";
  const isTypst = isDocument && engineOf(active.path) === "typst";
  const list: Command[] = [
    {
      id: "goto.file",
      title: "Go to file",
      group: "Workbench",
      keywords: "open quick find",
      shortcuts: [PALETTE_FILES_SHORTCUT],
      enabled: hasProject,
      run: () => openPalette("files"),
    },
    {
      id: "view.files",
      title: "Show files",
      group: "Workbench",
      keywords: "explorer tree project",
      shortcuts: [mod("e", { shift: true })],
      run: () => layout.showSide("files"),
    },
    {
      id: "view.search",
      title: "Search in project",
      group: "Workbench",
      keywords: "find text",
      shortcuts: [mod("f", { shift: true })],
      run: () => layout.showSide("search"),
    },
    {
      id: "view.figures",
      title: "Show figures",
      group: "Workbench",
      keywords: "draw diagrams",
      run: () => layout.showSide("figures"),
    },
    {
      id: "view.references",
      title: "Show references",
      group: "Workbench",
      keywords: "bibliography citations bib",
      run: () => layout.showSide("references"),
    },
    {
      id: "view.plugins",
      title: "Show plugins",
      group: "Workbench",
      keywords: "extensions commands permissions",
      shortcuts: [mod("x", { shift: true })],
      run: () => layout.showSide("plugins"),
    },
    {
      id: "view.settings",
      title: "Open settings",
      group: "Workbench",
      keywords: "theme preferences",
      shortcuts: [mod(",")],
      run: () => layout.showSide("settings"),
    },
    {
      id: "view.toggle-side",
      title: "Toggle side panel",
      group: "Workbench",
      shortcuts: [mod("b")],
      run: () =>
        layout.sideView === null ? layout.showSide("files") : layout.closeSide(),
    },
    {
      id: "view.toggle-panel",
      title: "Toggle bottom panel",
      group: "Workbench",
      keywords: "problems output",
      shortcuts: [mod("j")],
      run: () => layout.togglePanel(),
    },
    {
      id: "view.problems",
      title: "Show problems",
      group: "Workbench",
      keywords: "errors warnings diagnostics",
      shortcuts: [mod("m", { shift: true })],
      run: () => layout.showPanel("problems"),
    },
    {
      id: "view.output",
      title: "Show compile output",
      group: "Workbench",
      keywords: "log",
      run: () => layout.showPanel("output"),
    },
    {
      id: "view.preview-split",
      title: "Preview: editor and preview side by side",
      group: "Workbench",
      keywords: "split layout",
      run: () => layout.setPreview("split"),
    },
    {
      id: "view.preview-editor",
      title: "Preview: editor only",
      group: "Workbench",
      run: () => layout.setPreview("editor"),
    },
    {
      id: "view.preview-preview",
      title: "Preview: preview only",
      group: "Workbench",
      run: () => layout.setPreview("preview"),
    },
    {
      id: "view.cycle-preview",
      title: "Cycle preview layout",
      group: "Workbench",
      shortcuts: [mod("v", { shift: true })],
      run: () => layout.cyclePreview(),
    },
    {
      id: "view.zen",
      title: layout.zen ? "Leave zen mode" : "Enter zen mode",
      group: "Workbench",
      keywords: "focus distraction free fullscreen",
      shortcuts: [mod("\\")],
      enabled: isDocument,
      run: () => layout.toggleZen(),
    },
    {
      id: "view.theme",
      title: layout.theme === "dark" ? "Switch to light theme" : "Switch to dark theme",
      group: "Workbench",
      keywords: "dark light appearance",
      run: () => layout.setTheme(layout.theme === "dark" ? "light" : "dark"),
    },
    {
      id: "tab.next",
      title: "Next tab",
      group: "Workbench",
      shortcuts: [mod("PageDown"), mod("Tab")],
      enabled: tabs.tabs.length > 1,
      run: () => tabs.cycle(1),
    },
    {
      id: "tab.previous",
      title: "Previous tab",
      group: "Workbench",
      shortcuts: [mod("PageUp"), mod("Tab", { shift: true })],
      enabled: tabs.tabs.length > 1,
      run: () => tabs.cycle(-1),
    },
    {
      id: "tab.close",
      title: "Close tab",
      group: "Workbench",
      shortcuts: [mod("w")],
      enabled: active !== undefined,
      run: () => actions.closeActiveTab(),
    },
    {
      id: "tab.close-others",
      title: "Close other tabs",
      group: "Workbench",
      enabled: tabs.tabs.length > 1,
      run: () => {
        if (active !== undefined) void actions.closeOtherTabs(active.id);
      },
    },
    {
      id: "file.new",
      title: "New file",
      group: "Workbench",
      shortcuts: [mod("n")],
      enabled: hasProject,
      run: () => void actions.newFileHere(),
    },
    {
      id: "file.new-figure",
      title: "New figure",
      group: "Workbench",
      keywords: "draw diagram",
      enabled: hasProject,
      run: () => void actions.newFigure(),
    },
    {
      id: "file.save",
      title: "Save",
      group: "Workbench",
      shortcuts: [mod("s")],
      enabled: active !== undefined && actions.isTabDirty(active.path),
      run: () => void actions.saveActive(),
    },
    {
      id: "file.save-all",
      title: "Save all",
      group: "Workbench",
      shortcuts: [mod("s", { shift: true })],
      enabled:
        documents.dirtyPaths.length > 0 ||
        tabs.tabs.some((t) => actions.isTabDirty(t.path)),
      run: () => void actions.saveAll(),
    },
    {
      id: "project.close",
      title: "Close project",
      group: "Workbench",
      keywords: "welcome open folder recent",
      enabled: hasProject,
      run: () => void actions.closeProject(),
    },
    {
      id: "project.refresh",
      title: "Refresh files",
      group: "Workbench",
      enabled: hasProject,
      run: () => void workspace.refresh(),
    },
    {
      id: "doc.compile",
      title: "Compile now",
      group: "Workbench",
      keywords: "build refresh preview",
      enabled: hasProject,
      run: () => void build.compile(),
    },
    {
      id: "doc.export-pdf",
      title: "Export PDF",
      group: "Workbench",
      keywords: "publish print download",
      shortcuts: [mod("e")],
      enabled: isTypst,
      run: () => actions.exportPdf(),
    },
    {
      id: "doc.slides",
      title:
        active?.kind === "document" && active.view === "slides"
          ? "Show document as text"
          : "Show document as slides",
      group: "Workbench",
      keywords: "deck presentation",
      enabled: isTypst,
      run: () => actions.toggleSlides(),
    },
  ];
  return list;
}

/** Every shortcut, with the command it runs, for the global key handler. */
export function shortcutTable(
  commands: readonly Command[],
): { shortcut: Shortcut; run: () => void }[] {
  const table: { shortcut: Shortcut; run: () => void }[] = [];
  for (const command of commands) {
    if (command.enabled === false) continue;
    for (const shortcut of command.shortcuts ?? [])
      table.push({ shortcut, run: command.run });
  }
  return table;
}
