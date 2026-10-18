/**
 * Workbench actions: the operations that touch more than one store.
 *
 * The stores (workspace, tabs, documents, figures, build) each know one thing
 * and none imports another's actions, so cross-cutting behaviour such as "close
 * this tab, asking about unsaved changes" lives here. Components and commands
 * call these; they are the workbench's vocabulary.
 */

import { paletteAction } from "$lib/commands/allowlist";
import { recheckWriting, runFormat } from "$lib/editor/writing";
import { writingSyntaxFor } from "$lib/editor/writing/languageMap";
import { kernel, errorMessage, inTauri, reportError } from "$lib/kernel";
import type { KernelEvent } from "$lib/kernel";
import { projectApi, writingApi } from "$lib/kernel/api";
import type { Engine, ProjectChanged } from "$lib/kernel/types";
import type { Problem } from "$lib/workbench/problems";
import {
  hasNativeDialogs,
  openExternal,
  openWithSystem,
  pickFolder,
  pickSavePath,
  revealInFileManager,
  saveBinaryArtifact,
} from "$lib/kernel/native";
import { SESSION_KEY, loadJson, saveJson } from "$lib/workbench/persist";
import {
  basename,
  dirname,
  fileKind,
  join,
  nameProblem,
  newFileName,
  uniqueName,
} from "$lib/workbench/paths";
import { citationFor, type BibEntry } from "$lib/workbench/references";
import {
  formatSize,
  needsProbe,
  openAsByContent,
  openAsByName,
  tabKindOf,
  type OpenAs,
} from "$lib/workbench/opener";
import { sanitizeSession, toSession } from "$lib/workbench/tabs";

import { build } from "./build.svelte";
import { diagram } from "./diagram.svelte";
import { exportDiagramsOnSave } from "./diagramFiles";
import { dialog } from "./dialog.svelte";
import { documents } from "./documents.svelte";
import { blankFigure } from "./figure.svelte";
import { figures } from "./figures.svelte";
import { viewers } from "./viewers.svelte";
import { home } from "./home.svelte";
import { layout } from "./layout.svelte";
import { notice } from "./notice.svelte";
import { applyPreferences } from "./preferences.svelte";
import { preview } from "./preview.svelte";
import { settings } from "./settings.svelte";
import { slides } from "./slides.svelte";
import { tabs } from "./tabs.svelte";
import { workspace } from "./workspace.svelte";

/* -------------------------------------------------------------------------- */
/* Files and tabs                                                              */
/* -------------------------------------------------------------------------- */

export interface OpenOptions {
  /** Move keyboard focus into the editor afterwards. Default true. */
  readonly focus?: boolean;
  readonly view?: "text" | "slides";
  /** Open this way instead of deciding (an SVG opened as text, say). */
  readonly as?: OpenAs;
}

/**
 * How `path` opens (`workbench/opener.ts`): by its name, or by its first bytes
 * when the name says nothing or it may be large. The bytes, when read, are
 * returned so a viewer does not read the file twice.
 */
async function decideOpen(
  path: string,
): Promise<{ as: OpenAs; bytes?: Uint8Array } | null> {
  const named = openAsByName(path);
  if (named !== null && !needsProbe(path)) return { as: named };
  try {
    const bytes = await projectApi(kernel).readBytes(path);
    return { as: openAsByContent(path, bytes), bytes };
  } catch (error) {
    reportError("actions.openFile", error);
    notice.error(`Could not open ${path}: ${errorMessage(error)}`);
    return null;
  }
}

/** Open a file in a tab, or focus the tab it already has. Every file opens. */
export async function openFile(
  path: string,
  options: OpenOptions = {},
): Promise<boolean> {
  const open = tabs.tabs.find((t) => t.path === path);
  if (open !== undefined && options.as === undefined) {
    tabs.open(path, open.kind === "document" ? options.view : undefined);
    if (layout.narrow) layout.closeSide();
    if (options.focus !== false && open.kind === "document") documents.focusEditor();
    return true;
  }
  const decided =
    options.as === undefined ? await decideOpen(path) : { as: options.as };
  if (decided === null) return false;
  const { as, bytes } = decided;
  const kind = tabKindOf(as);
  const ok =
    kind === "figure"
      ? (await figures.open(path)) !== null
      : kind === "viewer"
        ? await viewers.open(path, as, bytes)
        : (await documents.open(path)) !== null;
  if (!ok) return false;
  if (as === "large-text") {
    notice.show(
      `${basename(path)} is ${formatSize(bytes?.length ?? 0)}, too large to edit here, so it opens read-only.`,
    );
  }
  tabs.open(path, kind === "document" ? options.view : undefined, kind);
  if (layout.narrow) layout.closeSide();
  if (options.focus !== false && kind === "document") documents.focusEditor();
  return true;
}

/** Reopen a viewer tab (an SVG) in the text editor, in the same place. */
export async function openAsText(path: string): Promise<void> {
  if (!(await documents.open(path))) return;
  tabs.reopenAs(path, "document");
  viewers.drop(path);
  documents.focusEditor();
}

/** "Open with system app" for a project file. False in the browser build. */
export async function openInSystemApp(path: string): Promise<void> {
  const root = workspace.project?.root;
  if (root === undefined) return;
  try {
    if (!(await openWithSystem(`${root}/${path}`))) {
      notice.show("Opening files in other apps needs the desktop app.");
    }
  } catch (error) {
    reportError("actions.openInSystemApp", error);
    notice.error(`Could not open ${basename(path)}: ${errorMessage(error)}`);
  }
}

/** "Show in file manager" for a project file. */
export async function revealFile(path: string): Promise<void> {
  const root = workspace.project?.root;
  if (root === undefined) return;
  try {
    if (!(await revealInFileManager(`${root}/${path}`))) {
      notice.show("Showing files in the file manager needs the desktop app.");
    }
  } catch (error) {
    reportError("actions.revealFile", error);
    notice.error(`Could not show ${basename(path)}: ${errorMessage(error)}`);
  }
}

export function isTabDirty(path: string): boolean {
  return documents.isDirty(path) || figures.isDirty(path);
}

async function save(path: string): Promise<boolean> {
  return fileKind(path) === "figure" ? figures.save(path) : documents.save(path);
}

export async function saveActive(): Promise<void> {
  const active = tabs.active;
  if (active === undefined) return;
  if (!isTabDirty(active.path)) return;
  if (await save(active.path)) notice.show(`Saved ${basename(active.path)}.`);
}

export async function saveAll(): Promise<void> {
  await documents.saveAll();
  for (const figure of figures.all()) if (figure.dirty) await figures.save(figure.path);
  notice.show("Saved all changes.");
}

/** Close a tab, asking first when it has unsaved changes. Returns false if cancelled. */
export async function closeTab(id: string): Promise<boolean> {
  const tab = tabs.tabs.find((t) => t.id === id);
  if (tab === undefined) return true;
  if (isTabDirty(tab.path)) {
    const choice = await dialog.ask({
      title: `Save changes to ${basename(tab.path)}?`,
      message: "Your changes will be lost if you close without saving.",
      actions: [
        { id: "save", label: "Save" },
        { id: "discard", label: "Don't save" },
      ],
    });
    if (choice === "cancel") return false;
    if (choice === "save" && !(await save(tab.path))) return false;
  }
  tabs.close(id);
  documents.drop(tab.path);
  figures.drop(tab.path);
  viewers.drop(tab.path);
  return true;
}

export async function closeOtherTabs(id: string): Promise<void> {
  for (const tab of [...tabs.tabs])
    if (tab.id !== id && !(await closeTab(tab.id))) return;
}

export async function closeTabsToRight(id: string): Promise<void> {
  const index = tabs.tabs.findIndex((t) => t.id === id);
  for (const tab of tabs.tabs.slice(index + 1)) if (!(await closeTab(tab.id))) return;
}

export function closeActiveTab(): void {
  const active = tabs.active;
  if (active !== undefined) void closeTab(active.id);
}

/* -------------------------------------------------------------------------- */
/* Projects                                                                    */
/* -------------------------------------------------------------------------- */

/** True when it is safe to discard every open document. */
async function confirmDiscard(): Promise<boolean> {
  const dirty = tabs.tabs.filter((t) => isTabDirty(t.path));
  if (dirty.length === 0) return true;
  const choice = await dialog.ask({
    title: `Save changes to ${dirty.length} ${dirty.length === 1 ? "file" : "files"}?`,
    message: "Your changes will be lost if you leave without saving.",
    actions: [
      { id: "save", label: "Save all" },
      { id: "discard", label: "Don't save" },
    ],
  });
  if (choice === "cancel") return false;
  if (choice === "save") await saveAll();
  return true;
}

function resetEditors(): void {
  for (const tab of tabs.tabs) {
    documents.drop(tab.path);
    figures.drop(tab.path);
    viewers.drop(tab.path);
  }
  tabs.clear();
  preview.release();
}

/** Tabs remembered per project, so switching back finds the same files open. */
const SESSIONS_KEY = "manang.sessions.v1";
const MAX_SESSIONS = 30;

function loadSessions(): Record<string, unknown> {
  const raw = loadJson(SESSIONS_KEY);
  return typeof raw === "object" && raw !== null
    ? (raw as Record<string, unknown>)
    : {};
}

/** Remember the open project's tabs before leaving it. */
function rememberSession(): void {
  const project = workspace.project;
  if (project === null) return;
  const sessions = loadSessions();
  delete sessions[project.root];
  sessions[project.root] = toSession(project.root, tabs.model);
  // Keep the most recently left projects only.
  for (const key of Object.keys(sessions).slice(0, -MAX_SESSIONS)) delete sessions[key];
  saveJson(SESSIONS_KEY, sessions);
}

async function restoreSession(project: { main: string; root: string }): Promise<void> {
  const saved = sanitizeSession(loadSessions()[project.root]);
  for (const entry of saved.tabs) {
    await openFile(entry.path, { focus: false, view: entry.view });
  }
  if (saved.active !== null) tabs.activate(saved.active);
  if (tabs.tabs.length === 0) await openFile(project.main, { focus: false });
}

async function startProject(
  open: () => Promise<{ main: string; root: string } | null>,
): Promise<boolean> {
  if (!(await confirmDiscard())) return false;
  rememberSession();
  resetEditors();
  const project = await open();
  if (project === null) {
    // The kernel keeps the project that was open; put its tabs back.
    const staying = workspace.project;
    if (staying !== null) await restoreSession(staying);
    return false;
  }
  home.hide();
  if (!layout.narrow) layout.showSide("files");
  await restoreSession(project);
  build.schedule("switch");
  void home.load();
  return true;
}

export function openProject(path: string): Promise<boolean> {
  return startProject(() => workspace.open(path));
}

export function createProject(
  path: string,
  name: string,
  engine: Engine,
): Promise<boolean> {
  return startProject(() => workspace.create(path, name, engine));
}

/** Show Home over the open project without closing it. */
/**
 * Open a project folder: the native picker on the desktop, otherwise (or if
 * the picker fails) the typed-path form on Home.
 */
export async function openFolder(): Promise<void> {
  workspace.error = "";
  home.show();
  if (hasNativeDialogs()) {
    try {
      const picked = await pickFolder("Open project folder");
      if (picked !== null) await openProject(picked);
      return;
    } catch {
      // The picker itself failed; offer the typed path instead.
    }
  }
  home.typingPath = true;
}

export function goHome(): void {
  home.show();
}

export async function closeProject(): Promise<void> {
  if (!(await confirmDiscard())) return;
  rememberSession();
  resetEditors();
  await workspace.close();
  workspace.error = "";
  tabs.remember();
  home.hide();
  void home.load();
  void build.compile();
}

/* -------------------------------------------------------------------------- */
/* Creating, renaming, deleting                                                */
/* -------------------------------------------------------------------------- */

const TEMPLATE: Record<string, (stem: string) => string> = {
  typst: (stem) => `= ${stem}\n\n`,
  latex: () => "",
  markdown: (stem) => `# ${stem}\n\n`,
};

/** Create a file in `dir` and open it. Returns an error sentence, or null on success. */
export async function createFile(dir: string, name: string): Promise<string | null> {
  const problem = nameProblem(name, workspace.namesIn(dir));
  if (problem !== null) return problem;
  const clean = name.trim();
  const kind = fileKind(clean);
  const stem = clean.replace(/\.[^.]+$/, "");
  const content = kind === "figure" ? blankFigure() : (TEMPLATE[kind]?.(stem) ?? "");
  try {
    const path = await workspace.createFile(dir, clean, content);
    await workspace.reveal(path);
    await openFile(path);
    return null;
  } catch (error) {
    return errorMessage(error);
  }
}

export async function createFolder(dir: string, name: string): Promise<string | null> {
  const problem = nameProblem(name, workspace.namesIn(dir));
  if (problem !== null) return problem;
  try {
    await workspace.createFolder(dir, name.trim());
    return null;
  } catch (error) {
    return errorMessage(error);
  }
}

/** A new, empty figure in `figures/`, opened for drawing. */
export async function newFigure(): Promise<void> {
  if (workspace.project === null) {
    notice.error("Open a folder before creating a figure.");
    return;
  }
  await workspace.loadDir("figures");
  const path = join(
    "figures",
    uniqueName("untitled.figure.json", workspace.namesIn("figures")),
  );
  try {
    // The figure plugin writes the scene and its SVG and keeps it open.
    await figures.create(path);
  } catch (error) {
    reportError("actions.newFigure", error);
    notice.error(`Could not create the figure: ${errorMessage(error)}`);
    return;
  }
  await workspace.loadDir("figures");
  await workspace.reveal(path);
  await openFile(path);
}

export async function newFileHere(): Promise<void> {
  if (workspace.project === null) {
    notice.error("Open a folder before creating a file.");
    return;
  }
  layout.showSide("files");
  const dir = "";
  const name = uniqueName(newFileName(workspace.engine), workspace.namesIn(dir));
  const problem = await createFile(dir, name);
  if (problem !== null) notice.error(`Could not create the file: ${problem}`);
}

/** Rename an entry. Returns an error sentence, or null on success. */
export async function renameEntry(
  path: string,
  newName: string,
): Promise<string | null> {
  const dir = dirname(path);
  const problem = nameProblem(newName, workspace.namesIn(dir), basename(path));
  if (problem !== null) return problem;
  const to = join(dir, newName.trim());
  if (to === path) return null;
  try {
    await workspace.rename(path, to);
  } catch (error) {
    return errorMessage(error);
  }
  tabs.renamed(path, to);
  documents.renamed(path, to);
  figures.renamed(path, to);
  viewers.renamed(path, to);
  return null;
}

export async function trashEntry(path: string): Promise<void> {
  const lost = tabs.tabs.filter(
    (t) => (t.path === path || t.path.startsWith(`${path}/`)) && isTabDirty(t.path),
  );
  try {
    await workspace.trash(path);
  } catch (error) {
    notice.error(
      `Could not move ${basename(path)} to the trash: ${errorMessage(error)}`,
    );
    return;
  }
  tabs.removed(path);
  documents.dropUnder(path);
  figures.dropUnder(path);
  viewers.dropUnder(path);
  notice.show(
    lost.length > 0
      ? `Moved ${basename(path)} to the trash. Unsaved changes in it were discarded.`
      : `Moved ${basename(path)} to the trash.`,
  );
  build.noteDiskChange();
}

/* -------------------------------------------------------------------------- */
/* Editor helpers                                                              */
/* -------------------------------------------------------------------------- */

export async function goToProblem(problem: Problem): Promise<void> {
  if (problem.file === "") return;
  if (!(await openFile(problem.file, { focus: false }))) return;
  documents.revealAt(problem.file, problem.line ?? 1, problem.column ?? 1);
  // The shown document may already be the right one; apply right away.
  documents.applyReveal();
}

export async function goToMatch(
  path: string,
  line: number,
  column: number,
): Promise<void> {
  if (!(await openFile(path, { focus: false }))) return;
  documents.revealAt(path, line, column);
  documents.applyReveal();
}

export function insertCitation(entry: BibEntry): void {
  const active = tabs.active;
  const engine = active === undefined ? null : workspace.engineFor(active.path);
  const text = citationFor(entry.key, engine);
  if (
    active?.kind === "document" &&
    active.view === "text" &&
    documents.insertAtCursor(text)
  ) {
    notice.show(`Inserted ${text}.`);
  } else {
    notice.error("Open a document and place the caret where the citation should go.");
  }
}

export function toggleSlides(): void {
  const active = tabs.active;
  if (active === undefined || active.kind !== "document") return;
  tabs.setView(active.id, active.view === "slides" ? "text" : "slides");
  slides.stop();
}

export function exportPdf(): void {
  const target = build.previewTarget() ?? tabs.active?.path ?? "document";
  if (hasNativeDialogs()) void exportPdfNative(basename(target));
  else void preview.exportPdf(basename(target));
}

/** Desktop: choose where in a native Save dialog, then the shell writes the PDF. */
async function exportPdfNative(name: string): Promise<void> {
  const stem = name.replace(/\.[^.]+$/, "");
  const root = workspace.project?.root;
  preview.exporting = true;
  try {
    const dest = await pickSavePath(
      root === undefined ? `${stem}.pdf` : `${root}/${stem}.pdf`,
      { name: "PDF document", extensions: ["pdf"] },
    );
    if (dest === null) return;
    await build.compile();
    if (build.status === "failed") {
      notice.error("Fix the errors in the Problems panel before exporting a PDF.");
      return;
    }
    // The engine that compiled the preview exports it: Typst from its last
    // compile, LaTeX from the build directory of the main file.
    if (build.engine === "latex")
      await saveBinaryArtifact(
        "latex.export-pdf",
        { main: join(root ?? "", build.target) },
        dest,
      );
    else await saveBinaryArtifact("typst.export-pdf", null, dest);
    notice.show(`Exported ${basename(dest)}.`);
  } catch (error) {
    notice.error(`Could not export a PDF: ${errorMessage(error)}`);
  } finally {
    preview.exporting = false;
  }
}

/** Format the shown document with the writing plugin (one undo step). */
export function formatDocument(): void {
  const view = documents.view;
  if (view === undefined || documents.shown === undefined) {
    notice.error("Open a document to format it.");
    return;
  }
  void runFormat(view, undefined, writingSyntaxFor(documents.shown.language));
}

/** Forget cached spelling and lint answers and check the visible text again. */
export function recheckSpelling(): void {
  const view = documents.view;
  if (view === undefined || documents.shown === undefined) {
    notice.error("Open a document to check its spelling.");
    return;
  }
  recheckWriting(view);
  notice.show("Checking spelling.");
}

/** Count the shown document and say the result in the status line. */
export async function showStats(): Promise<void> {
  const session = documents.shown;
  if (session === undefined) {
    notice.error("Open a document to count its words.");
    return;
  }
  try {
    const stats = await writingApi(kernel).stats(
      session.text(),
      writingSyntaxFor(session.language),
    );
    notice.show(
      `${stats.words} words, ${stats.characters} characters, ${stats.sentences} sentences.`,
    );
  } catch (error) {
    notice.error(`Could not count: ${errorMessage(error)}`);
  }
}

/** Run a kernel command picked from the palette. */
export async function runKernelCommand(id: string): Promise<void> {
  // Commands that need the editor run the shell's own action, not a bare call.
  switch (paletteAction(id)) {
    case "format":
      formatDocument();
      return;
    case "spell":
      recheckSpelling();
      return;
    case "stats":
      await showStats();
      return;
    case "export-pdf":
      exportPdf();
      return;
    case "new-figure":
      await newFigure();
      return;
    case null:
      break;
  }
  try {
    await kernel.run(id);
    notice.show(`${id} finished.`);
  } catch (error) {
    notice.error(`${id} could not run: ${errorMessage(error)}`);
  }
}

/* -------------------------------------------------------------------------- */
/* Start-up and kernel events                                                  */
/* -------------------------------------------------------------------------- */

function onKernelEvent(event: KernelEvent): void {
  const project = workspace.project;
  if (event.topic === "project.watch-error") {
    const message = (event.payload as { message?: unknown } | null)?.message;
    notice.error(
      `File watching stopped: ${typeof message === "string" ? message : "unknown error"}`,
    );
    return;
  }
  if (event.topic !== "project.changed" || project === null) return;
  const payload = event.payload as ProjectChanged | null;
  if (payload === null || payload.root !== project.root) return;
  workspace.scheduleRefresh();
  const touched = payload.changes
    .filter((c) => c.kind === "modified" || c.kind === "created")
    .map((c) => c.path);
  void documents.externalChange(touched);
  void viewers.externalChange(touched);
  build.noteDiskChange();
}

let booted = false;

/** Restore layout, the last project and its tabs, and start listening to the kernel. */
/**
 * Web links anywhere in the shell (a DOI, a licence) open in the default
 * browser instead of replacing the app. One listener, so no component needs
 * its own handling.
 */
function routeExternalLinks(): void {
  document.addEventListener("click", (event) => {
    const link = (event.target as HTMLElement | null)?.closest?.("a[href]");
    if (!(link instanceof HTMLAnchorElement) || !/^https?:/i.test(link.href)) return;
    event.preventDefault();
    openExternal(link.href).catch((error: unknown) =>
      notice.error(errorMessage(error)),
    );
  });
}

/**
 * Closing the window (the title bar's close button, Alt+F4, the dock) asks
 * about unsaved changes first, then remembers the session and closes.
 */
async function guardWindowClose(): Promise<void> {
  if (!inTauri()) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  const win = getCurrentWindow();
  let closing = false;
  await win.onCloseRequested(async (event) => {
    if (closing) return;
    event.preventDefault();
    if (!(await confirmDiscard())) return;
    rememberSession();
    closing = true;
    await win.destroy();
  });
}

export async function boot(): Promise<void> {
  if (booted) return;
  booted = true;
  layout.restore();
  home.restore();
  applyPreferences();
  void settings.load(null);
  void guardWindowClose();
  routeExternalLinks();
  documents.onEdit(() => build.schedule("edit"));
  diagram.start();
  exportDiagramsOnSave();
  void kernel.onEvent(onKernelEvent);
  await workspace.loadRecent();
  void home.load();

  const session = sanitizeSession(loadJson(SESSION_KEY));
  if (session.project === null) return;
  if ((await workspace.open(session.project)) === null) {
    // The folder moved or was deleted; fall back to Home.
    workspace.error = "";
    return;
  }
  for (const entry of session.tabs) {
    await openFile(entry.path, { focus: false, view: entry.view });
  }
  if (session.active !== null) tabs.activate(session.active);
  else if (tabs.tabs.length === 0)
    await openFile(workspace.project?.main ?? "", { focus: false });
  build.schedule("switch");
}
