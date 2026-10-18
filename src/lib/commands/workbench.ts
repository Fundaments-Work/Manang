/**
 * Workbench commands: panels, tabs, files and documents.
 *
 * One list drives both the palette and the global keyboard handler, so a
 * shortcut shown beside a command is by construction the one that runs it.
 * Commands are rebuilt each time the palette opens so `enabled` is current.
 */

import { build } from "$lib/state/build.svelte";
import { documents } from "$lib/state/documents.svelte";
import { home } from "$lib/state/home.svelte";
import { layout } from "$lib/state/layout.svelte";
import { settings } from "$lib/state/settings.svelte";
import { settingsPage } from "$lib/state/settingsPage.svelte";
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

/**
 * Where each command applies (`when` over the UI context keys, see
 * `workbench/context.ts`). Commands not listed apply everywhere.
 */
const WHEN: Readonly<Record<string, string>> = {
  "goto.file": "project",
  "view.files": "project",
  "view.search": "project",
  "view.figures": "project",
  "view.references": "project",
  "view.toggle-side": "project",
  "view.toggle-panel": "project",
  "view.problems": "project",
  "view.output": "project",
  "project.refresh": "project",
  "doc.compile": "project",
  "file.new": "project",
  "file.new-figure": "project",
  "view.preview-split": "workbench && document",
  "view.preview-editor": "workbench && document",
  "view.preview-preview": "workbench && document",
  "view.cycle-preview": "workbench && document",
  "view.zen": "workbench && document",
  "tab.next": "workbench",
  "tab.previous": "workbench",
  "tab.close": "workbench",
  "tab.close-others": "workbench",
  "file.save": "workbench",
  "file.save-all": "project",
  "doc.export-pdf": "workbench && document",
  "doc.format": "workbench && document",
  "doc.slides": "workbench && typst",
  "project.close": "project",
};

/**
 * Commands that act on the workbench bring it forward first, so running
 * "Show files" from Home or Settings shows the files rather than nothing.
 */
const BRINGS_WORKBENCH: ReadonlySet<string> = new Set([
  "goto.file",
  "view.files",
  "view.search",
  "view.figures",
  "view.references",
  "view.toggle-side",
  "view.toggle-panel",
  "view.problems",
  "view.output",
  "file.new",
  "file.new-figure",
]);

function toWorkbench(): void {
  settingsPage.hide();
  if (workspace.project !== null) home.hide();
}

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
      run: () => settingsPage.show("plugins"),
    },
    {
      id: "view.settings",
      title: "Open settings",
      group: "Workbench",
      keywords: "theme preferences",
      shortcuts: [mod(",")],
      run: () => settingsPage.show(),
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
      run: () =>
        void settings.set(
          "appearance.theme",
          layout.theme === "dark" ? "light" : "dark",
          "user",
        ),
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
      id: "doc.format",
      title: "Format document",
      group: "Workbench",
      keywords: "prettify tidy indent whitespace writing.format.document",
      // Handled by the editor's own keymap; listed here so the palette shows it.
      shortcuts: [{ key: "f", shift: true, alt: true }],
      enabled: isDocument && active.view === "text",
      run: () => actions.formatDocument(),
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
    {
      id: "home.show",
      title: "Go to Home",
      group: "Workbench",
      keywords: "projects recent welcome",
      run: () => {
        settingsPage.hide();
        home.show();
      },
    },
    {
      id: "home.new-project",
      title: "New project",
      group: "Workbench",
      keywords: "create start template",
      run: () => {
        settingsPage.hide();
        home.show();
        home.openNew();
      },
    },
  ];
  return list.map((command) => ({
    ...command,
    when: WHEN[command.id],
    run: BRINGS_WORKBENCH.has(command.id)
      ? () => {
          toWorkbench();
          command.run();
        }
      : command.run,
  }));
}

/** Every shortcut, with the command it runs, for the global key handler. */
export function shortcutTable(
  commands: readonly Command[],
  applies: (when: string | undefined) => boolean = () => true,
): { shortcut: Shortcut; run: () => void }[] {
  const table: { shortcut: Shortcut; run: () => void }[] = [];
  for (const command of commands) {
    if (command.enabled === false || !applies(command.when)) continue;
    for (const shortcut of command.shortcuts ?? [])
      table.push({ shortcut, run: command.run });
  }
  return table;
}
