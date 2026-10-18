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
  indentUnit,
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

import { aiDiff } from "./aiDiff";
import { problemMarkers } from "./gutter";
import { writing } from "./writing";
import { languageFor, loadLanguageExtension, type EditorLanguage } from "./languages";
import { manangTheme } from "./theme";

export { languageFor, type EditorLanguage };

/** Shared by every state: each state holds its own content for it. */
export const languageCompartment = new Compartment();

/**
 * The user's editor preferences that need CodeMirror's help: the gutter,
 * wrapping and indentation. Font size and line height are CSS variables set
 * on the document root (see `preferences.svelte.ts`), so they never cost a
 * reconfigure. One extension value is shared by every document; a document
 * created or shown later picks up the current one through `syncEditorPrefs`.
 */
export const prefsCompartment = new Compartment();

/**
 * Holds the live-session binding of a document (`editor/live/binding.ts`):
 * empty until the document joins a session, empty again after it leaves.
 */
export const liveCompartment = new Compartment();

export interface EditorLayoutPrefs {
  readonly tabSize: number;
  readonly wordWrap: boolean;
  readonly lineNumbers: boolean;
}

function prefsExtension(prefs: EditorLayoutPrefs): Extension {
  return [
    prefs.lineNumbers ? [lineNumbers(), highlightActiveLineGutter()] : [],
    prefs.wordWrap ? EditorView.lineWrapping : [],
    EditorState.tabSize.of(prefs.tabSize),
    indentUnit.of(" ".repeat(prefs.tabSize)),
  ];
}

let currentPrefs: EditorLayoutPrefs = { tabSize: 2, wordWrap: true, lineNumbers: true };
let currentPrefsExtension: Extension = prefsExtension(currentPrefs);

/** Change the preferences; returns false when nothing changed. */
export function setEditorPrefs(prefs: EditorLayoutPrefs): boolean {
  if (
    prefs.tabSize === currentPrefs.tabSize &&
    prefs.wordWrap === currentPrefs.wordWrap &&
    prefs.lineNumbers === currentPrefs.lineNumbers
  )
    return false;
  currentPrefs = prefs;
  currentPrefsExtension = prefsExtension(prefs);
  return true;
}

/** Bring a view's current state up to the latest preferences, if it is behind. */
export function syncEditorPrefs(view: EditorView): void {
  if (prefsCompartment.get(view.state) === currentPrefsExtension) return;
  view.dispatch({ effects: prefsCompartment.reconfigure(currentPrefsExtension) });
}

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
      prefsCompartment.of(currentPrefsExtension),
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
      liveCompartment.of([]),
      manangTheme,
      problemMarkers(),
      writing(),
      aiDiff(),
      extra,
    ],
  });
}

export function createEditorView(parent: HTMLElement, state: EditorState): EditorView {
  return new EditorView({ parent, state });
}
