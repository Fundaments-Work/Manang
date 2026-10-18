import { basename } from "./paths";

export interface TitleContext {
  /** Where the person is: Home, Settings, or the workbench. */
  readonly surface: "home" | "settings" | "workbench";
  /** The open project's name, if any. */
  readonly project: string | null;
  /** The active tab's project-relative path, if any. */
  readonly file: string | null;
}

/**
 * The window title, which the desktop shows in the taskbar and alt-tab list:
 * "main.typ — Entropy paper" in a project, "Settings" or "Home" on those
 * pages, and just "Manang" when nothing is open. It replaces the breadcrumb
 * the top bar used to carry, which could not be clicked.
 */
export function windowTitle({ surface, project, file }: TitleContext): string {
  if (surface === "settings")
    return project === null ? "Settings" : `Settings — ${project}`;
  if (surface === "home" || project === null)
    return surface === "home" ? "Home" : "Manang";
  return file === null ? project : `${basename(file)} — ${project}`;
}
