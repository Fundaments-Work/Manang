/**
 * The surfaces the shell registers itself with the focused-surface
 * dispatcher (`$lib/commands/surface`): the CodeMirror editor, and plain text
 * fields as the native fallback. The figure canvas and the PDF viewer
 * register themselves where they live.
 *
 * Also follows focus (`focusin`) so an Edit command run from a menu still
 * reaches the surface that had focus before the menu took it.
 */

import { redo, selectAll, undo } from "@codemirror/commands";
import { openSearchPanel } from "@codemirror/search";
import type { EditorView } from "@codemirror/view";

import { focusedSurface } from "$lib/commands/surface";
import { documents } from "$lib/state/documents.svelte";

/** Elements inside the menu bar never count as "where focus was". */
const MENU_ROOT = "[data-menu-root]";

function liveView(): EditorView | null {
  const view = documents.view;
  return view !== undefined && view.dom.isConnected ? view : null;
}

async function readClipboard(): Promise<string> {
  return navigator.clipboard.readText();
}

/** Copy or cut the editor's selection through the clipboard API. */
async function editorClipboard(view: EditorView, cut: boolean): Promise<void> {
  const { state } = view;
  const text = state.selection.ranges
    .filter((r) => !r.empty)
    .map((r) => state.sliceDoc(r.from, r.to))
    .join(state.lineBreak);
  if (text === "") return;
  await navigator.clipboard.writeText(text);
  if (cut) view.dispatch(state.replaceSelection(""));
}

function editorReplace(view: EditorView): void {
  openSearchPanel(view);
  const field = view.dom.querySelector<HTMLInputElement>(
    ".cm-search input[name=replace]",
  );
  field?.focus();
}

function isTextField(
  element: Element,
): element is HTMLInputElement | HTMLTextAreaElement {
  if (element instanceof HTMLTextAreaElement) return !element.readOnly;
  if (!(element instanceof HTMLInputElement)) return false;
  return (
    !element.readOnly &&
    ["text", "search", "url", "email", "tel", "password", "number", ""].includes(
      element.type,
    )
  );
}

/** Run a native editing command on the focused field. */
function exec(command: "undo" | "redo" | "cut" | "copy"): void {
  // execCommand is deprecated but remains the only way to reach a field's
  // own undo stack and clipboard handling from script.
  document.execCommand(command);
}

let installed = false;

/** Register the editor and native fields, and start following focus. Once. */
export function installShellSurfaces(): void {
  if (installed || typeof document === "undefined") return;
  installed = true;

  focusedSurface.register({
    id: "editor",
    contains: (el) => liveView()?.dom.contains(el as Node) ?? false,
    available: () => liveView() !== null,
    actions: {
      undo: () => {
        const view = liveView();
        if (view !== null) undo(view);
      },
      redo: () => {
        const view = liveView();
        if (view !== null) redo(view);
      },
      selectAll: () => {
        const view = liveView();
        if (view !== null) selectAll(view);
      },
      copy: async () => {
        const view = liveView();
        if (view !== null) await editorClipboard(view, false);
      },
      cut: async () => {
        const view = liveView();
        if (view !== null) await editorClipboard(view, true);
      },
      paste: async () => {
        const view = liveView();
        if (view === null) return;
        const text = await readClipboard();
        view.dispatch(view.state.replaceSelection(text));
      },
      find: () => {
        const view = liveView();
        if (view !== null) openSearchPanel(view);
      },
      replace: () => {
        const view = liveView();
        if (view !== null) editorReplace(view);
      },
    },
  });

  const field = (): HTMLInputElement | HTMLTextAreaElement | null => {
    const el = document.activeElement;
    return el !== null && isTextField(el) ? el : null;
  };
  focusedSurface.register({
    id: "native",
    fallback: true,
    contains: (el) => el instanceof Element && isTextField(el),
    actions: {
      undo: () => exec("undo"),
      redo: () => exec("redo"),
      cut: () => exec("cut"),
      copy: () => exec("copy"),
      paste: async () => {
        const target = field();
        if (target === null) return;
        const text = await readClipboard();
        // insertText keeps the field's own undo history.
        if (!document.execCommand("insertText", false, text)) target.setRangeText(text);
      },
      selectAll: () => field()?.select(),
    },
  });

  document.addEventListener(
    "focusin",
    (event) => {
      const target = event.target;
      if (target instanceof Element && target.closest(MENU_ROOT) === null)
        focusedSurface.noteFocus(target);
    },
    { capture: true },
  );
}
