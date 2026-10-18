/**
 * Convert a Markdown file to Typst with the markdown plugin
 * (`markdown.to-typst`): the result goes next to it as `<name>.typ`, through
 * `project.write-file`, and opens in a tab. An existing file is replaced only
 * after asking.
 */

import { errorMessage, kernel, reportError } from "$lib/kernel";
import { markdownApi, type MarkdownApi } from "$lib/kernel/api";
import { convertedMessage, typstPathFor } from "$lib/workbench/convert";
import { basename, dirname } from "$lib/workbench/paths";

import { openFile } from "./actions";
import { dialog } from "./dialog.svelte";
import { documents } from "./documents.svelte";
import { notice } from "./notice.svelte";
import { workspace } from "./workspace.svelte";

let api: MarkdownApi | undefined;

/**
 * Convert the Markdown file at `path` (its unsaved text when open) and open
 * the Typst file. Returns the new path, or null when cancelled or failed.
 */
export async function convertToTypst(path: string): Promise<string | null> {
  const target = typstPathFor(path);
  if (target === null) {
    notice.error(`${basename(path)} is not a Markdown file.`);
    return null;
  }
  const exists =
    workspace.paths.includes(target) || documents.get(target) !== undefined;
  if (exists) {
    const choice = await dialog.ask({
      title: `Replace ${basename(target)}?`,
      message: `${basename(target)} already exists next to ${basename(path)}. Converting replaces its contents.`,
      actions: [{ id: "replace", label: "Replace" }],
    });
    if (choice !== "replace") return null;
  }
  try {
    const source = documents.get(path)?.text() ?? (await workspace.readFile(path));
    api ??= markdownApi(kernel);
    const result = await api.toTypst(source);
    await workspace.createFile(dirname(target), basename(target), result.typst);
    // An open tab of the old file would otherwise keep showing it.
    documents.drop(target);
    await workspace.reveal(target);
    await openFile(target);
    notice.show(convertedMessage(target, result.diagnostics));
    return target;
  } catch (error) {
    reportError(`could not convert ${path} to Typst`, error);
    notice.error(`Could not convert ${basename(path)}: ${errorMessage(error)}`);
    return null;
  }
}
