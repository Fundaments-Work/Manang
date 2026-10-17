/**
 * Editor construction.
 *
 * CodeMirror owns its DOM subtree entirely: the manuscript never re-renders
 * through Svelte, so keystroke latency is a function of CodeMirror and the Rust
 * buffer alone. AGENTS.md's <8 ms budget is protected by keeping the framework
 * out of the typing path.
 *
 * One `EditorView` serves every open document. Each document keeps its own
 * `EditorState` (text, undo history, selection, problem markers) and the view
 * swaps between them with `setState`, so switching tabs costs one state swap
 * rather than a new editor.
 *
 * Language support loads lazily through a Compartment. The Typst grammar ships a
 * 367 KB WebAssembly parser; blocking first paint on it would trade the app's
 * headline metric for syntax colour, so a document opens as plain text and
 * upgrades in place a moment later.
 */

import {
  autocompletion,
  closeBrackets,
  closeBracketsKeymap,
  completionKeymap,
} from "@codemirror/autocomplete";
import {
  defaultKeymap,
  history,
  historyKeymap,
  indentWithTab,
} from "@codemirror/commands";
import {
  bracketMatching,
  foldGutter,
  foldKeymap,
  indentOnInput,
} from "@codemirror/language";
import { highlightSelectionMatches, search, searchKeymap } from "@codemirror/search";
import { Compartment, EditorState, type Extension } from "@codemirror/state";
import {
  EditorView,
  drawSelection,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSpecialChars,
  keymap,
  lineNumbers,
  rectangularSelection,
} from "@codemirror/view";

import { problemMarkers } from "./gutter";
import {
  languageFor,
  loadLanguageExtension,
  type EditorLanguage,
} from "./languages";
import { manangTheme } from "./theme";

export { languageFor, type EditorLanguage };

/** Shared by every state: each state holds its own content for it. */
export const languageCompartment = new Compartment();

const loaded = new Map<EditorLanguage, Promise<Extension>>();

/**
 * Loads a language layer on demand, once per language.
 *
 * The grammar and its highlight style arrive in the same chunk as each other.
 * That coupling matters: importing the grammar's tag table at module scope
 * would pull the Typst package, including its WebAssembly loader, into the
 * initial bundle and defeat the code split entirely.
 */
export function loadLanguage(language: EditorLanguage): Promise<Extension> {
  let promise = loaded.get(language);
  if (promise === undefined) {
    promise = loadLanguageExtension(language);
    loaded.set(language, promise);
  }
  return promise;
}

/** A fresh editor state for one document. `extra` carries the per-document hooks. */
export function createEditorState(text: string, extra: Extension): EditorState {
  return EditorState.create({
    doc: text,
    extensions: [
      lineNumbers(),
      highlightActiveLineGutter(),
      highlightSpecialChars(),
      history(),
      foldGutter(),
      drawSelection(),
      indentOnInput(),
      bracketMatching(),
      closeBrackets(),
      rectangularSelection(),
      highlightActiveLine(),
      highlightSelectionMatches(),
      search({ top: true }),
      autocompletion({ activateOnTyping: true }),
      EditorView.lineWrapping,
      EditorView.contentAttributes.of({
        "aria-label": "Document text",
        "aria-multiline": "true",
      }),
      keymap.of([
        ...closeBracketsKeymap,
        ...defaultKeymap,
        ...searchKeymap,
        ...historyKeymap,
        ...foldKeymap,
        ...completionKeymap,
        indentWithTab,
      ]),
      languageCompartment.of([]),
      manangTheme,
      problemMarkers(),
      extra,
    ],
  });
}

export function createEditorView(parent: HTMLElement, state: EditorState): EditorView {
  return new EditorView({ parent, state });
}
