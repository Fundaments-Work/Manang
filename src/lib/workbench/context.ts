/**
 * The UI context: where the person is and what they are working on, as one
 * value every part of the shell reads instead of each re-deriving it.
 *
 * The top bar picks its search target and actions from it, the command
 * palette and the keyboard map use it to show and run only what applies
 * (VS Code-style `when` clauses), and plugins see the same context keys the
 * keybindings registry uses. Pure, so the rules are unit-tested.
 */

import type { Engine } from "./paths";

/** The full-window surface on screen. */
export type Surface = "home" | "settings" | "workbench" | "help";

/** What the editor area shows, when the workbench is on screen. */
export type EditorKind = "document" | "figure" | "slides";

export interface UiContext {
  readonly surface: Surface;
  readonly project: boolean;
  readonly editor: EditorKind | null;
  readonly engine: Engine | null;
  readonly zen: boolean;
}

/** Names usable in `when` clauses. */
export type ContextKey = Surface | EditorKind | Engine | "project" | "zen";

export function contextKeys(context: UiContext): ReadonlySet<ContextKey> {
  const keys = new Set<ContextKey>([context.surface]);
  if (context.project) keys.add("project");
  if (context.zen) keys.add("zen");
  // Settings keeps the context it was opened over (a Typst document's
  // settings still see `document` and `typst`); Home has none.
  if (context.surface !== "home") {
    if (context.editor !== null) keys.add(context.editor);
    if (context.engine !== null) keys.add(context.engine);
  }
  return keys;
}

/**
 * Evaluate a `when` clause such as `workbench && document`,
 * `typst || latex` or `!home`. `||` binds looser than `&&`, `!` negates one
 * key; no parentheses. An absent or empty clause always matches; an unknown
 * key is simply false.
 */
export function whenMatches(
  when: string | undefined,
  keys: ReadonlySet<string>,
): boolean {
  if (when === undefined || when.trim() === "") return true;
  return when.split("||").some((any) =>
    any.split("&&").every((term) => {
      const t = term.trim();
      if (t.startsWith("!")) return !keys.has(t.slice(1).trim());
      return keys.has(t);
    }),
  );
}
