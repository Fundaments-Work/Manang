/**
 * Source and preview: Ctrl+click (Cmd+click) on a rendered page opens the
 * source there, and "Show in preview" outlines where the caret's text landed.
 *
 * Typst answers through `typst.jump-from-click` / `typst.jump-from-cursor`,
 * LaTeX through `latex.synctex`. Both speak points from the top-left corner of
 * a page, which is what the preview measures clicks in.
 */

import { errorMessage, kernel, reportError } from "$lib/kernel";
import { latexApi, typstApi } from "$lib/kernel/api";
import { openExternal } from "$lib/kernel/native";
import { flashLine } from "$lib/editor/flashLine";
import { basename, engineOf, join } from "$lib/workbench/paths";
import { latexSourcePath, synctexFile, typstMarkBox } from "$lib/workbench/sourceJump";

import { goToMatch } from "./actions";
import { build } from "./build.svelte";
import { documents } from "./documents.svelte";
import { layout } from "./layout.svelte";
import { notice } from "./notice.svelte";
import { preview } from "./preview.svelte";
import { tabs } from "./tabs.svelte";
import { workspace } from "./workspace.svelte";

const typst = typstApi(kernel);
const latex = latexApi(kernel);

/** Open `path` at a 1-based line and column and flash that line. */
async function openAt(path: string, line: number, column: number): Promise<void> {
  await goToMatch(path, line, column);
  // The tab may still be switching; flash once the editor shows the file.
  for (let frame = 0; frame < 30; frame += 1) {
    const view = documents.view;
    if (
      view !== undefined &&
      documents.shown?.path === path &&
      documents.revealRequest === null
    ) {
      flashLine(view, view.state.selection.main.head);
      return;
    }
    await new Promise((resolve) => requestAnimationFrame(resolve));
  }
}

/**
 * Ctrl+click on the preview: `page` is 0-based, `x`/`y` in points from the
 * page's top-left corner.
 */
export async function jumpToSource(page: number, x: number, y: number): Promise<void> {
  try {
    if (build.engine === "typst") {
      const jump = await typst.jumpFromClick(page, x, y);
      if (jump === null) {
        notice.show("Nothing in the source maps to that spot. Ctrl+click on text.");
      } else if (jump.kind === "file") {
        if (jump.path.startsWith("@")) {
          notice.show(
            `That text comes from the package ${jump.path.split("/")[1] ?? jump.path}.`,
          );
        } else {
          await openAt(jump.path, jump.line, jump.column);
        }
      } else if (jump.kind === "url") {
        await openExternal(jump.url);
      } else {
        preview.showMark(typstMarkBox(jump, preview.pageWidth(jump.page)));
      }
      return;
    }
    if (build.engine === "latex") {
      const root = workspace.project?.root ?? "";
      const target = preview.shownTarget || build.target;
      const at = await latex.synctexReverse(join(root, target), page + 1, x, y);
      const path = latexSourcePath(at.file, target, root);
      if (path === null) {
        notice.show(`That text comes from ${basename(at.file)}, outside the project.`);
        return;
      }
      await openAt(path, at.line, 1);
    }
  } catch (error) {
    reportError("preview.jump-to-source", error);
    notice.error(`Could not find the source for that spot: ${errorMessage(error)}`);
  }
}

/** "Show in preview": outline where the caret's text appears on the pages. */
export async function showInPreview(): Promise<void> {
  const active = tabs.active;
  if (active?.kind !== "document") {
    notice.show("Open a Typst or LaTeX document to show its text in the preview.");
    return;
  }
  const engine = engineOf(active.path);
  if (engine !== "typst" && engine !== "latex") {
    notice.show("Show in preview works in Typst and LaTeX documents.");
    return;
  }
  if (layout.preview === "editor") layout.setPreview("split");
  if (build.generation === 0 || build.engine !== engine) {
    notice.show("Compile the document first; the preview has nothing to show yet.");
    return;
  }
  const { line, column } = documents.cursor;
  try {
    if (engine === "typst") {
      const points = await typst.jumpFromCursor(active.path, line, column);
      const first = points[0];
      if (first === undefined) {
        notice.show("The text at the caret does not appear on the pages.");
        return;
      }
      preview.showMark(typstMarkBox(first, preview.pageWidth(first.page)));
      return;
    }
    const root = workspace.project?.root ?? "";
    const target = preview.shownTarget || build.target;
    const found = await latex.synctexForward(
      join(root, target),
      synctexFile(active.path, target, root),
      line,
    );
    const box = found.positions[0];
    if (box === undefined) {
      notice.show("The text at the caret does not appear on the pages.");
      return;
    }
    preview.showMark({
      page: box.page - 1,
      x: box.x,
      y: box.y,
      width: box.width,
      height: Math.max(box.height, 4),
    });
  } catch (error) {
    reportError("preview.show-in-preview", error);
    notice.error(`Could not show this line in the preview: ${errorMessage(error)}`);
  }
}
