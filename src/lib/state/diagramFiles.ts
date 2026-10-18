/**
 * Diagram file actions for the palette: create a new diagram from a template,
 * and copy the line that includes the active diagram in the main document.
 */

import { errorMessage, reportError } from "$lib/kernel";
import {
  DIAGRAM_KINDS,
  diagramLanguageOf,
  diagramNameProblem,
  diagramPath,
  includeLine,
} from "$lib/workbench/diagram";
import { basename } from "$lib/workbench/paths";

import { openFile } from "./actions";
import { dialog } from "./dialog.svelte";
import { notice } from "./notice.svelte";
import { tabs } from "./tabs.svelte";
import { workspace } from "./workspace.svelte";

const DIR = "figures";

/** Ask for a name and a kind, write `figures/<name>.<ext>` and open it. Never overwrites. */
export async function newDiagram(): Promise<void> {
  if (workspace.project === null) {
    notice.error("Open a folder before creating a diagram.");
    return;
  }
  let name = "diagram";
  for (;;) {
    const answer = await dialog.prompt({
      title: "New diagram",
      message: `A diagram file in ${DIR}/, drawn beside its source as you type. Pick its kind.`,
      input: { label: "Name", value: name },
      actions: DIAGRAM_KINDS.map((k) => ({ id: k.id, label: k.label })),
    });
    const kind = DIAGRAM_KINDS.find((k) => k.id === answer.id);
    if (kind === undefined) return;
    name = answer.value;
    const problem = diagramNameProblem(name);
    if (problem !== null) {
      notice.error(problem);
      continue;
    }
    const path = diagramPath(name, kind);
    if (workspace.namesIn("").includes(DIR)) await workspace.loadDir(DIR);
    if (workspace.namesIn(DIR).includes(basename(path))) {
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
      await workspace.createFile(DIR, basename(path), kind.template);
      await workspace.reveal(path);
      await openFile(path);
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

/** Copy the include line for the active diagram, for the project's engine. */
export async function copyIncludeLine(): Promise<void> {
  const active = tabs.active;
  if (active === undefined || !diagramIsActive()) {
    notice.error("Open a diagram file first.");
    return;
  }
  const engine = workspace.project?.engine ?? workspace.engine;
  const line = includeLine(engine, active.path);
  const picture = engine === "latex" ? "PDF" : "SVG";
  try {
    await navigator.clipboard.writeText(line);
    notice.show(
      `Copied ${line}. Manang does not write the ${picture} next to the diagram yet; that comes with the document integration.`,
    );
  } catch (error) {
    reportError("copy include line", error);
    notice.error(`Could not reach the clipboard. Type this line instead: ${line}`);
  }
}
