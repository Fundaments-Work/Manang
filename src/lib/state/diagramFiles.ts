/**
 * Diagram files: create a new diagram from a template, write its rendered
 * picture next to it on every save (`project.export-diagram`: a print-ready
 * SVG, plus a PDF in LaTeX projects), and copy the line that includes it in a
 * document.
 */

import { errorMessage, kernel, reportError } from "$lib/kernel";
import { projectApi } from "$lib/kernel/api";
import {
  DIAGRAM_DIR,
  chosenKind,
  diagramLanguageOf,
  diagramNameProblem,
  diagramPath,
  includeLine,
  newDiagramRequest,
} from "$lib/workbench/diagram";
import { basename } from "$lib/workbench/paths";

import { openFile } from "./actions";
import { dialog } from "./dialog.svelte";
import { documents } from "./documents.svelte";
import { notice } from "./notice.svelte";
import { tabs } from "./tabs.svelte";
import { workspace } from "./workspace.svelte";

const project = projectApi(kernel);

/** Ask for a name and a kind, write `figures/<name>.<ext>` and open it. Never overwrites. */
export async function newDiagram(): Promise<void> {
  if (workspace.project === null) {
    notice.error("Open a folder before creating a diagram.");
    return;
  }
  let name = "diagram";
  let kindId = "";
  for (;;) {
    const answer = await dialog.prompt(newDiagramRequest(name, kindId));
    const kind = chosenKind(answer);
    if (kind === null) return;
    name = answer.value;
    kindId = kind.id;
    const problem = diagramNameProblem(name);
    if (problem !== null) {
      notice.error(problem);
      continue;
    }
    const path = diagramPath(name, kind);
    if (workspace.namesIn("").includes(DIAGRAM_DIR))
      await workspace.loadDir(DIAGRAM_DIR);
    if (workspace.namesIn(DIAGRAM_DIR).includes(basename(path))) {
      const choice = await dialog.ask({
        title: `${path} already exists`,
        message:
          "Manang never overwrites a file. Open the existing one, or pick another name.",
        actions: [
          { id: "rename", label: "Choose another name" },
          { id: "open", label: "Open it" },
        ],
      });
      if (choice === "open") await openFile(path);
      if (choice === "rename") continue;
      return;
    }
    try {
      await workspace.createFile(DIAGRAM_DIR, basename(path), kind.template);
      await workspace.reveal(path);
      await openFile(path);
      await exportDiagram(path);
    } catch (error) {
      reportError("new diagram", error);
      notice.error(`Could not create the diagram: ${errorMessage(error)}`);
    }
    return;
  }
}

/** True when the active tab is a diagram file. */
export function diagramIsActive(): boolean {
  const active = tabs.active;
  return active?.kind === "document" && diagramLanguageOf(active.path) !== null;
}

/**
 * Write the rendered files of the diagram at `path` (what is on disk). A
 * source with errors keeps the previous files: Problems already shows why.
 * Returns false when the files could not be brought up to date.
 */
export async function exportDiagram(path: string): Promise<boolean> {
  try {
    const result = await project.exportDiagram(path);
    return result.ok;
  } catch (error) {
    reportError("diagram export", error);
    notice.error(
      `Could not write the picture of ${basename(path)}: ${errorMessage(error)}`,
    );
    return false;
  }
}

/** Keep every diagram's rendered files current. Call once at boot. */
export function exportDiagramsOnSave(): void {
  documents.onSave((path) => {
    if (diagramLanguageOf(path) !== null) void exportDiagram(path);
  });
}

/** Copy the include line for the active diagram, for the project's engine. */
export async function copyIncludeLine(): Promise<void> {
  const active = tabs.active;
  if (active === undefined || !diagramIsActive()) {
    notice.error("Open a diagram file first.");
    return;
  }
  const engine = workspace.project?.engine ?? workspace.engine;
  const line = includeLine(engine, active.path);
  // Make sure the picture the line names exists (the saved source's).
  const current = await exportDiagram(active.path);
  try {
    await navigator.clipboard.writeText(line);
    notice.show(
      current && !documents.isDirty(active.path)
        ? `Copied ${line}`
        : `Copied ${line}. Save the diagram (without errors) to update its picture.`,
    );
  } catch (error) {
    reportError("copy include line", error);
    notice.error(`Could not reach the clipboard. Type this line instead: ${line}`);
  }
}
