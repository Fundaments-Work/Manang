/**
 * The project's main file: the document that `#include` / `\input` chapters are
 * compiled through. It lives in `manang.toml` (`[project] main`); the open
 * project already carries the resolved value (`workspace.project.main`).
 */

import { errorMessage } from "$lib/kernel";
import { basename, engineOf } from "$lib/workbench/paths";
import { setMainInToml } from "$lib/workbench/target";

import { build } from "./build.svelte";
import { notice } from "./notice.svelte";
import { workspace } from "./workspace.svelte";

const CONFIG = "manang.toml";

/** Can this file be the project's main document? (Same engine as the project.) */
export function canBeMain(path: string): boolean {
  const project = workspace.project;
  return project !== null && engineOf(path) === project.engine && path !== project.main;
}

/** Make `path` the main file: writes `manang.toml` and recompiles through it. */
export async function setMainFile(path: string): Promise<boolean> {
  const project = workspace.project;
  if (project === null) {
    notice.error("Open a folder before choosing a main file.");
    return false;
  }
  if (engineOf(path) !== project.engine) {
    notice.error(
      `${basename(path)} is not a ${project.engine} document, so it cannot be this project's main file.`,
    );
    return false;
  }
  try {
    const toml = await workspace.readFile(CONFIG);
    await workspace.writeFile(CONFIG, setMainInToml(toml, path));
  } catch (error) {
    notice.error(`Could not set the main file: ${errorMessage(error)}`);
    return false;
  }
  workspace.project = {
    ...project,
    main: path,
    config: { ...project.config, project: { ...project.config.project, main: path } },
  };
  notice.show(`${basename(path)} is now the main file.`);
  build.schedule("switch");
  return true;
}
