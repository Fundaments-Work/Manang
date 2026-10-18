/**
 * Workbench commands: panels, tabs, files, documents, editing and help.
 *
 * One list drives the palette, the menu bar and the global keyboard handler,
 * so a shortcut shown beside a command is by construction the one that runs
 * it. Shortcuts come from the keybindings registry (`keymap`), which holds the
 * defaults and the person's changes; commands whose keys the focused surface
 * handles itself (Undo, Copy, Find) declare theirs for display only.
 * Commands are rebuilt each time they are needed so `enabled` is current.
 */

import { agent } from "$lib/state/agent.svelte";
import { build } from "$lib/state/build.svelte";
import { context } from "$lib/state/context.svelte";
import { convertToTypst } from "$lib/state/convert";
import { copyIncludeLine, diagramIsActive, newDiagram } from "$lib/state/diagramFiles";
import { documents } from "$lib/state/documents.svelte";
import { figures } from "$lib/state/figures.svelte";
import { helpPage } from "$lib/state/helpPage.svelte";
import { home } from "$lib/state/home.svelte";
import { layout } from "$lib/state/layout.svelte";
import { live } from "$lib/state/live.svelte";
import { scm } from "$lib/state/scm.svelte";
import { settings } from "$lib/state/settings.svelte";
import { settingsPage } from "$lib/state/settingsPage.svelte";
import { sharePopover } from "$lib/state/sharePopover.svelte";
import { shortcutOverlay } from "$lib/state/shortcutOverlay.svelte";
import { showInPreview } from "$lib/state/sourceJump";
import { switcher } from "$lib/state/switcher.svelte";
import { tabs } from "$lib/state/tabs.svelte";
import { saveVersion } from "$lib/state/versionActions";
import { workspace } from "$lib/state/workspace.svelte";
import * as actions from "$lib/state/actions";
import { feedback } from "$lib/state/feedback.svelte";
import { canOpenDevtools, inTauri, openDevtools } from "$lib/kernel/log";
import { engineOf } from "$lib/workbench/paths";
import { settingsOriginFor } from "$lib/workbench/settings";

import { keymap } from "./keymap.svelte";
import { withShortcuts } from "./keymap";
import type { MenuBarId, MenuPlacement } from "./menu";
import type { Command } from "./registry";
import type { Shortcut } from "./shortcut";
import { focusedSurface, type EditAction } from "./surface";

const mod = (key: string, extra: Partial<Shortcut> = {}): Shortcut => ({
  key,
  mod: true,
  ...extra,
});

const at = (bar: MenuBarId, group: string, order = 0): MenuPlacement => ({
  bar,
  group,
  order,
});

/**
 * Where each command applies (`when` over the UI context keys, see
 * `workbench/context.ts`). Commands not listed apply everywhere.
 */
const WHEN: Readonly<Record<string, string>> = {
  "goto.file": "project",
  "view.files": "project",
  "view.search": "project",
  "view.source-control": "project",
  "view.versions": "project",
  "file.save-version": "project",
  "view.outline": "project",
  "view.figures": "project",
  "view.references": "project",
  "view.toggle-side": "project",
  "view.toggle-panel": "project",
  "view.problems": "project",
  "view.output": "project",
  "agent.show": "project",
  "agent.toggle": "project",
  "agent.rewrite": "workbench && document",
  "agent.shorten": "workbench && document",
  "agent.translate": "workbench && document",
  "agent.explain": "workbench && document",
  "project.refresh": "project",
  "doc.compile": "project",
  "file.new": "project",
  "file.new-figure": "project",
  "file.new-diagram": "project",
  "doc.copy-include": "workbench && document",
  "figure.zoom-fit": "workbench && figure",
  "figure.zoom-selection": "workbench && figure",
  "figure.insert-mermaid": "workbench && figure",
  "figure.group": "workbench && figure",
  "figure.ungroup": "workbench && figure",
  "figure.front": "workbench && figure",
  "figure.back": "workbench && figure",
  "figure.lock": "workbench && figure",
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
  "share.open": "project",
  "live.share": "workbench && document",
  "live.join-session": "project",
  "live.copy-invite": "workbench",
  "live.leave-session": "workbench",
  "doc.export-pdf": "workbench && document",
  "doc.format": "workbench && document",
  "doc.show-in-preview": "workbench && document",
  "doc.slides": "workbench && typst",
  "doc.present": "workbench && typst",
  "doc.convert-typst": "workbench && markdown",
  "project.close": "project",
  ...Object.fromEntries(
    Array.from({ length: 9 }, (_, i) => [`tab.go-${i + 1}`, "workbench"]),
  ),
};

/**
 * Commands that act on the workbench bring it forward first, so running
 * "Show files" from Home or Settings shows the files rather than nothing.
 */
const BRINGS_WORKBENCH: ReadonlySet<string> = new Set([
  "goto.file",
  "view.files",
  "view.search",
  "view.source-control",
  "view.versions",
  "file.save-version",
  "view.outline",
  "view.figures",
  "view.references",
  "view.toggle-side",
  "view.toggle-panel",
  "view.problems",
  "view.output",
  "agent.show",
  "agent.toggle",
  "agent.rewrite",
  "agent.shorten",
  "agent.translate",
  "agent.explain",
  "file.new",
  "file.new-figure",
  "file.new-diagram",
]);

/**
 * The figure canvas's actions as commands (palette and View menu), for the
 * figure in the active tab. Their keys are handled by the canvas itself.
 */
function figureCommands(path: string | undefined): Command[] {
  const figure = path === undefined ? undefined : figures.get(path);
  const on = figure !== undefined;
  const selected = (figure?.selection.length ?? 0) > 0;
  const entry = (
    id: string,
    title: string,
    run: (f: NonNullable<typeof figure>) => void,
    enabled = on,
    extra: Partial<Command> = {},
  ): Command => ({
    id,
    title,
    group: "Workbench",
    keywords: "figure canvas",
    enabled,
    keys: "surface",
    run: () => {
      if (figure !== undefined) run(figure);
    },
    ...extra,
  });
  return [
    entry("figure.zoom-fit", "Figure: Zoom to fit", (f) => f.zoomToFit(), on, {
      shortcuts: [{ key: "1", shift: true }],
      menu: at("view", "7_figure", 0),
      menuTitle: "Zoom figure to fit",
    }),
    entry(
      "figure.zoom-selection",
      "Figure: Zoom to selection",
      (f) => f.zoomToSelection(),
      selected,
      {
        shortcuts: [{ key: "2", shift: true }],
      },
    ),
    entry(
      "figure.insert-mermaid",
      "Figure: Insert Mermaid",
      (f) => {
        f.mermaidDialog = true;
      },
      on,
      {
        keywords: "figure mermaid flowchart diagram shapes",
        menu: at("view", "7_figure", 1),
      },
    ),
    entry("figure.group", "Figure: Group", (f) => f.group(), selected, {
      shortcuts: [{ key: "g", mod: true }],
    }),
    entry("figure.ungroup", "Figure: Ungroup", (f) => f.ungroup(), selected, {
      shortcuts: [{ key: "g", mod: true, shift: true }],
    }),
    entry(
      "figure.front",
      "Figure: Bring to front",
      (f) => f.reorder("front"),
      selected,
      {
        shortcuts: [{ key: "]", mod: true, shift: true }],
      },
    ),
    entry("figure.back", "Figure: Send to back", (f) => f.reorder("back"), selected, {
      shortcuts: [{ key: "[", mod: true, shift: true }],
    }),
    entry("figure.lock", "Figure: Lock or unlock", (f) => f.toggleLock(), selected, {
      shortcuts: [{ key: "l", mod: true, shift: true }],
    }),
  ];
}

function toWorkbench(): void {
  settingsPage.hide();
  if (workspace.project !== null) home.hide();
}

/** The element focus was on, for deciding which Edit commands can run. */
function focused(): Element | null {
  return typeof document === "undefined" ? null : document.activeElement;
}

interface EditSpec {
  readonly action: EditAction;
  readonly title: string;
  readonly keywords?: string;
  /** Absent when the surfaces have no common key for it (Replace). */
  readonly shortcut?: Shortcut;
  readonly menu: MenuPlacement;
}

const EDITS: readonly (EditSpec & { readonly id: string })[] = [
  {
    id: "edit.undo",
    action: "undo",
    title: "Undo",
    shortcut: mod("z"),
    menu: at("edit", "1_undo", 0),
  },
  {
    id: "edit.redo",
    action: "redo",
    title: "Redo",
    shortcut: mod("z", { shift: true }),
    menu: at("edit", "1_undo", 1),
  },
  {
    id: "edit.cut",
    action: "cut",
    title: "Cut",
    shortcut: mod("x"),
    menu: at("edit", "2_clipboard", 0),
  },
  {
    id: "edit.copy",
    action: "copy",
    title: "Copy",
    shortcut: mod("c"),
    menu: at("edit", "2_clipboard", 1),
  },
  {
    id: "edit.paste",
    action: "paste",
    title: "Paste",
    shortcut: mod("v"),
    menu: at("edit", "2_clipboard", 2),
  },
  {
    id: "edit.select-all",
    action: "selectAll",
    title: "Select all",
    shortcut: mod("a"),
    menu: at("edit", "3_select", 0),
  },
  {
    id: "edit.find",
    action: "find",
    title: "Find",
    keywords: "search document",
    shortcut: mod("f"),
    menu: at("edit", "4_find", 0),
  },
  {
    id: "edit.replace",
    action: "replace",
    title: "Replace",
    keywords: "find substitute",
    menu: at("edit", "4_find", 1),
  },
];

function editCommands(): Command[] {
  const where = focused();
  return EDITS.map((spec) => ({
    id: spec.id,
    title: spec.title,
    group: "Workbench",
    ...(spec.keywords === undefined ? {} : { keywords: spec.keywords }),
    shortcuts: spec.shortcut === undefined ? [] : [spec.shortcut],
    keys: "surface",
    menu: spec.menu,
    enabled: focusedSurface.can(spec.action, where),
    run: () => actions.editAction(spec.action),
  }));
}

/**
 * The AI commands: the Agent view and the four selection actions (Edit > AI). They only open the
 * view and attach the selection; the request is sent by the action itself, once, and the result
 * comes back as an inline diff the person accepts or rejects.
 */
function aiCommands(isDocument: boolean): Command[] {
  const action = (
    id: string,
    title: string,
    command: "rewrite" | "shorten" | "translate" | "explain",
    order: number,
    keywords: string,
  ): Command => ({
    id,
    title,
    menuTitle: title.replace(/^AI: /, ""),
    group: "Workbench",
    keywords: `ai assistant ${keywords}`,
    enabled: isDocument,
    menu: at("edit", "5_ai", order),
    run: () => void agent.runCommand(command),
  });
  return [
    {
      id: "agent.show",
      title: "Show AI agent",
      menuTitle: "AI agent",
      group: "Workbench",
      keywords: "ai assistant chat model",
      run: () => agent.reveal(),
    },
    {
      id: "agent.toggle",
      title: "Toggle AI agent",
      menuTitle: "Agent",
      group: "Workbench",
      keywords: "ai assistant chat model",
      menu: at("view", "2_side", 7),
      run: () => {
        if (layout.agentOpen) layout.closeAgent();
        else agent.reveal();
      },
    },
    action(
      "agent.rewrite",
      "AI: Rewrite selection",
      "rewrite",
      0,
      "improve edit clarity",
    ),
    action("agent.shorten", "AI: Shorten selection", "shorten", 1, "condense concise"),
    action("agent.translate", "AI: Translate selection", "translate", 2, "language"),
    action(
      "agent.explain",
      "AI: Explain selection",
      "explain",
      3,
      "what does this mean",
    ),
  ];
}

export function workbenchCommands(
  openPalette: (mode: "commands" | "files") => void,
): Command[] {
  const hasProject = workspace.project !== null;
  const active = tabs.active;
  const isDocument = active?.kind === "document";
  const engine = isDocument ? engineOf(active.path) : null;
  const isTypst = engine === "typst";
  const isMarkdown = engine === "markdown";
  // A PDF comes from Typst or LaTeX; a Markdown preview has no PDF to export.
  const isPaged = engine === "typst" || engine === "latex";
  const list: Command[] = [
    {
      id: "palette.commands",
      title: "Command palette",
      group: "Workbench",
      keywords: "run search actions",
      menu: at("view", "1_palette", 0),
      run: () => openPalette("commands"),
    },
    {
      id: "goto.file",
      title: "Go to file",
      group: "Workbench",
      keywords: "open quick find",
      enabled: hasProject,
      menu: at("go", "1_file", 0),
      run: () => openPalette("files"),
    },
    {
      id: "view.files",
      title: "Show files",
      menuTitle: "Files",
      group: "Workbench",
      keywords: "explorer tree project",
      menu: at("view", "2_side", 0),
      run: () => layout.showSide("files"),
    },
    {
      id: "view.search",
      title: "Search in project",
      menuTitle: "Search",
      group: "Workbench",
      keywords: "find text",
      menu: [at("view", "2_side", 1), at("edit", "4_find", 2)],
      run: () => layout.showSide("search"),
    },
    {
      id: "view.source-control",
      title: "Show source control",
      menuTitle: "Source control",
      group: "Workbench",
      keywords: "git changes commit diff",
      menu: at("view", "2_side", 2),
      run: () => layout.showSide("source-control"),
    },
    {
      id: "view.versions",
      title: "Show versions",
      menuTitle: "Versions",
      group: "Workbench",
      keywords: "history restore earlier saved timeline",
      enabled: hasProject,
      menu: at("view", "2_side", 2.5),
      run: () => {
        scm.show("versions");
        layout.showSide("source-control");
      },
    },
    {
      id: "view.outline",
      title: "Show outline",
      menuTitle: "Outline",
      group: "Workbench",
      keywords: "headings sections structure",
      menu: at("view", "2_side", 3),
      run: () => layout.showSide("outline"),
    },
    {
      id: "view.figures",
      title: "Show figures",
      menuTitle: "Figures",
      group: "Workbench",
      keywords: "draw diagrams",
      menu: at("view", "2_side", 4),
      run: () => layout.showSide("figures"),
    },
    {
      id: "view.references",
      title: "Show references",
      menuTitle: "References",
      group: "Workbench",
      keywords: "bibliography citations bib",
      menu: at("view", "2_side", 5),
      run: () => layout.showSide("references"),
    },
    {
      id: "view.plugins",
      title: "Show plugins",
      menuTitle: "Plugins",
      group: "Workbench",
      keywords: "extensions commands permissions",
      menu: at("view", "2_side", 6),
      // A side view of the workbench; without a project there is no
      // workbench, so the Settings page's plugin sections stand in.
      run: () => {
        if (workspace.project === null) settingsPage.show("plugins");
        else {
          toWorkbench();
          layout.showSide("plugins");
        }
      },
    },
    {
      id: "view.settings",
      title: "Open settings",
      menuTitle: "Settings",
      group: "Workbench",
      keywords: "theme preferences",
      menu: at("file", "6_settings", 0),
      // Scoped to where you are: a document's settings over a document tab.
      run: () => settingsPage.show(null, context.settingsOrigin),
    },
    {
      id: "settings.all",
      title: "Open all settings",
      menuTitle: "All settings",
      group: "Workbench",
      keywords: "theme preferences application",
      menu: at("file", "6_settings", 1),
      run: () => settingsPage.show(),
    },
    {
      id: "settings.document",
      title: "Document settings",
      group: "Workbench",
      keywords: "preferences editor preview compile engine",
      enabled: isDocument,
      menu: at("file", "6_settings", 2),
      run: () => {
        if (active !== undefined)
          settingsPage.show(null, settingsOriginFor(active, false));
      },
    },
    {
      id: "view.toggle-side",
      title: "Toggle side panel",
      group: "Workbench",
      menu: at("view", "3_panels", 0),
      run: () =>
        layout.sideView === null ? layout.showSide("files") : layout.closeSide(),
    },
    {
      id: "view.toggle-panel",
      title: "Toggle bottom panel",
      menuTitle: "Toggle panel",
      group: "Workbench",
      keywords: "problems output",
      menu: at("view", "3_panels", 1),
      run: () => layout.togglePanel(),
    },
    {
      id: "view.problems",
      title: "Show problems",
      menuTitle: "Problems",
      group: "Workbench",
      keywords: "errors warnings diagnostics",
      menu: at("build", "3_panels", 0),
      run: () => layout.showPanel("problems"),
    },
    {
      id: "view.output",
      title: "Show compile output",
      menuTitle: "Output",
      group: "Workbench",
      keywords: "log",
      menu: at("build", "3_panels", 1),
      run: () => layout.showPanel("output"),
    },
    {
      id: "view.preview-split",
      title: "Preview: editor and preview side by side",
      menuTitle: "Editor and preview",
      group: "Workbench",
      keywords: "split layout",
      menu: at("view", "4_preview", 0),
      run: () => layout.setPreview("split"),
    },
    {
      id: "view.preview-editor",
      title: "Preview: editor only",
      menuTitle: "Editor only",
      group: "Workbench",
      menu: at("view", "4_preview", 1),
      run: () => layout.setPreview("editor"),
    },
    {
      id: "view.preview-preview",
      title: "Preview: preview only",
      menuTitle: "Preview only",
      group: "Workbench",
      menu: at("view", "4_preview", 2),
      run: () => layout.setPreview("preview"),
    },
    {
      id: "view.cycle-preview",
      title: "Cycle preview layout",
      group: "Workbench",
      menu: at("view", "4_preview", 3),
      run: () => layout.cyclePreview(),
    },
    {
      id: "view.zen",
      title: layout.zen ? "Leave zen mode" : "Enter zen mode",
      menuTitle: "Zen mode",
      group: "Workbench",
      keywords: "focus distraction free fullscreen",
      enabled: isDocument,
      menu: at("view", "5_appearance", 0),
      run: () => layout.toggleZen(),
    },
    {
      id: "view.theme",
      title: layout.theme === "dark" ? "Switch to light theme" : "Switch to dark theme",
      group: "Workbench",
      keywords: "dark light appearance",
      menu: at("view", "5_appearance", 1),
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
      enabled: tabs.tabs.length > 1,
      menu: at("go", "2_tabs", 0),
      run: () => tabs.cycle(1),
    },
    {
      id: "tab.previous",
      title: "Previous tab",
      group: "Workbench",
      enabled: tabs.tabs.length > 1,
      menu: at("go", "2_tabs", 1),
      run: () => tabs.cycle(-1),
    },
    {
      id: "tab.close",
      title: "Close tab",
      group: "Workbench",
      enabled: active !== undefined,
      menu: at("go", "3_close", 0),
      run: () => actions.closeActiveTab(),
    },
    {
      id: "tab.close-others",
      title: "Close other tabs",
      group: "Workbench",
      enabled: tabs.tabs.length > 1,
      menu: at("go", "3_close", 1),
      run: () => {
        if (active !== undefined) void actions.closeOtherTabs(active.id);
      },
    },
    ...Array.from({ length: 9 }, (_, i): Command => ({
      id: `tab.go-${i + 1}`,
      title: `Go to tab ${i + 1}`,
      group: "Workbench",
      palette: false,
      // Go to tab 9 is the last tab, as in other editors.
      enabled: i === 8 ? tabs.tabs.length > 0 : tabs.tabs.length > i,
      run: () => tabs.byShortcut(i + 1),
    })),
    {
      id: "home.new-project",
      title: "New project",
      group: "Workbench",
      keywords: "create start template",
      menu: at("file", "1_new", 0),
      run: () => {
        settingsPage.hide();
        home.show();
        home.openNew();
      },
    },
    {
      id: "file.new",
      title: "New file",
      group: "Workbench",
      enabled: hasProject,
      menu: at("file", "1_new", 1),
      run: () => void actions.newFileHere(),
    },
    {
      id: "file.new-figure",
      title: "New figure",
      group: "Workbench",
      keywords: "draw diagram sketch figure.create",
      enabled: hasProject,
      menu: at("file", "1_new", 2),
      run: () => void actions.newFigure(),
    },
    {
      id: "file.new-diagram",
      title: "New diagram",
      group: "Workbench",
      keywords: "mermaid pikchr ascii svgbob flowchart sequence",
      enabled: hasProject,
      menu: at("file", "1_new", 3),
      run: () => void newDiagram(),
    },
    {
      id: "project.open-folder",
      title: "Open folder…",
      group: "Workbench",
      keywords: "project open browse directory",
      menu: at("file", "2_open", 0),
      run: () => void actions.openFolder(),
    },
    {
      id: "project.switch",
      title: "Switch project…",
      group: "Workbench",
      keywords: "recent projects open",
      menu: at("file", "2_open", 1),
      run: () => void switcher.show(),
    },
    {
      id: "doc.copy-include",
      title: "Copy include line",
      group: "Workbench",
      keywords: "diagram image includegraphics insert",
      enabled: diagramIsActive(),
      menu: at("edit", "5_document", 1),
      run: () => void copyIncludeLine(),
    },
    ...figureCommands(active?.path),
    {
      id: "file.save",
      title: "Save",
      group: "Workbench",
      enabled: active !== undefined && actions.isTabDirty(active.path),
      menu: at("file", "3_save", 0),
      run: () => void actions.saveActive(),
    },
    {
      id: "file.save-all",
      title: "Save all",
      group: "Workbench",
      enabled:
        documents.dirtyPaths.length > 0 ||
        tabs.tabs.some((t) => actions.isTabDirty(t.path)),
      menu: at("file", "3_save", 1),
      run: () => void actions.saveAll(),
    },
    {
      id: "file.save-version",
      title: "Save a version",
      group: "Workbench",
      keywords: "commit snapshot history keep name",
      enabled: hasProject,
      menu: at("file", "3_save", 2),
      run: () => void saveVersion(),
    },
    {
      id: "share.open",
      title: "Share",
      menuTitle: "Share…",
      group: "Workbench",
      keywords: "collaborate together live invite versions get updates co-author",
      enabled: hasProject,
      menu: at("file", "4_live", -1),
      run: () => {
        toWorkbench();
        sharePopover.toggle();
      },
    },
    {
      id: "live.share",
      title: "Share this file live",
      menuTitle: "Share this file live…",
      group: "Workbench",
      keywords: "collaborate together real time co-edit invite session",
      enabled: isDocument && active.view === "text" && !live.active && !live.busy,
      menu: at("file", "4_live", 0),
      run: () => void live.share(),
    },
    {
      id: "live.join-session",
      title: "Join a live session",
      menuTitle: "Join a live session…",
      group: "Workbench",
      keywords: "collaborate together real time invite paste",
      enabled: hasProject && !live.active && !live.busy,
      menu: at("file", "4_live", 1),
      run: () => void live.join(),
    },
    {
      id: "live.copy-invite",
      title: "Copy live invite",
      group: "Workbench",
      keywords: "share link session",
      enabled: live.active,
      menu: at("file", "4_live", 2),
      run: () => void live.copyInvite(),
    },
    {
      id: "live.leave-session",
      title: "Leave live session",
      group: "Workbench",
      keywords: "stop end session collaborate",
      enabled: live.active,
      menu: at("file", "4_live", 3),
      run: () => void live.leave(),
    },
    {
      id: "project.close",
      title: "Close project",
      group: "Workbench",
      keywords: "welcome open folder recent",
      enabled: hasProject,
      menu: at("file", "5_close", 0),
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
      menuTitle: "Compile",
      group: "Workbench",
      keywords: "build refresh preview",
      enabled: hasProject,
      menu: at("build", "1_compile", 0),
      run: () => void build.compile(),
    },
    {
      id: "doc.export-pdf",
      title: "Export PDF",
      group: "Workbench",
      keywords: "publish print download",
      enabled: isPaged,
      menu: [at("file", "4_export", 0), at("build", "1_compile", 1)],
      run: () => actions.exportPdf(),
    },
    {
      id: "doc.format",
      title: "Format document",
      group: "Workbench",
      keywords: "prettify tidy indent whitespace writing.format.document",
      enabled: isDocument && active.view === "text",
      menu: at("edit", "5_document", 0),
      run: () => actions.formatDocument(),
    },
    {
      id: "doc.show-in-preview",
      title: "Show in preview",
      group: "Workbench",
      keywords: "jump sync synctex forward search locate caret cursor",
      enabled: isPaged && active?.kind === "document" && active.view === "text",
      menu: at("go", "4_source", 0),
      run: () => void showInPreview(),
    },
    {
      id: "doc.slides",
      title:
        active?.kind === "document" && active.view === "slides"
          ? "Show document as text"
          : "Show document as slides",
      menuTitle: "Slides",
      group: "Workbench",
      keywords: "deck presentation",
      enabled: isTypst,
      menu: at("view", "5_appearance", 2),
      run: () => actions.toggleSlides(),
    },
    {
      id: "doc.present",
      title: "Present slides",
      group: "Workbench",
      keywords: "deck presentation fullscreen slideshow",
      enabled: isTypst,
      menu: at("build", "2_present", 0),
      run: () => actions.presentSlides(),
    },
    {
      id: "doc.convert-typst",
      title: "Convert to Typst",
      group: "Workbench",
      keywords: "markdown export typ markdown.to-typst",
      enabled: isMarkdown,
      menu: at("edit", "5_document", 2),
      run: () => {
        if (active !== undefined) void convertToTypst(active.path);
      },
    },
    {
      id: "home.show",
      title: "Go to Home",
      menuTitle: "Home",
      group: "Workbench",
      keywords: "projects recent welcome",
      menu: at("go", "1_file", 1),
      run: () => actions.toggleHome(),
    },
    {
      id: "help.user-guide",
      title: "User guide",
      group: "Workbench",
      keywords: "help docs manual documentation guide",
      menu: at("help", "1_reference", 0),
      run: () => helpPage.show("index"),
    },
    {
      id: "help.shortcuts",
      title: "Keyboard shortcuts",
      group: "Workbench",
      keywords: "keys cheat sheet reference bindings",
      menu: at("help", "1_reference", 1),
      run: () => shortcutOverlay.toggle(),
    },
    {
      id: "help.keybindings",
      title: "Change keyboard shortcuts",
      group: "Workbench",
      keywords: "keybindings rebind customise",
      menu: at("help", "1_reference", 2),
      run: () => settingsPage.show("keyboard"),
    },
    {
      id: "help.report-problem",
      title: "Report a problem",
      group: "Workbench",
      keywords: "bug issue log crash diagnostics",
      menu: at("help", "2_problem", 0),
      run: () => void feedback.show("problem"),
    },
    {
      id: "help.send-feedback",
      title: "Send feedback",
      menuTitle: "Send feedback…",
      group: "Workbench",
      keywords: "feedback suggestion idea tester beta",
      menu: at("help", "2_problem", 1),
      run: () => void feedback.show("feedback"),
    },
    {
      id: "help.open-logs",
      title: "Open logs folder",
      menuTitle: "Open logs",
      group: "Workbench",
      keywords: "log diagnostics",
      menu: at("help", "2_problem", 2),
      run: () => void actions.revealLogs(false),
    },
    {
      id: "dev.inspect",
      title: "Toggle developer tools",
      group: "Workbench",
      keywords: "inspector devtools debug",
      enabled: canOpenDevtools(),
      menu: at("help", "3_dev", 0),
      run: () => openDevtools(),
    },
    {
      id: "help.about",
      title: "About Manang",
      group: "Workbench",
      keywords: "version licence",
      menu: at("help", "9_about", 0),
      run: () => settingsPage.show("about"),
    },
    {
      id: "app.quit",
      title: "Quit",
      group: "Workbench",
      keywords: "exit close window",
      enabled: inTauri(),
      menu: at("file", "9_quit", 0),
      run: () => actions.quitApp(),
    },
    ...aiCommands(isDocument),
  ];
  const wrapped = list.map((command) => ({
    ...command,
    when: WHEN[command.id],
    run: BRINGS_WORKBENCH.has(command.id)
      ? () => {
          toWorkbench();
          command.run();
        }
      : command.run,
  }));
  return [...withShortcuts(wrapped, keymap.byCommand), ...editCommands()];
}
