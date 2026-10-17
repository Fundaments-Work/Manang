/**
 * Editor construction.
 *
 * CodeMirror owns this DOM subtree entirely, which is the point: the manuscript
 * never re-renders through Svelte, so keystroke latency is a function of
 * CodeMirror and the Rust buffer alone. AGENTS.md's <8 ms budget is protected by
 * keeping the framework out of the typing path.
 *
 * Language support is loaded lazily and injected through a Compartment. The
 * Typst grammar ships a 367 KB WebAssembly parser; blocking first paint on it
 * would trade the app's headline metric for syntax colour, so the editor opens
 * as plain text and upgrades in place a moment later.
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
  syntaxHighlighting,
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

import type { DocumentLanguage } from "$lib/state/document.svelte";
import { pushDiagnostics, whispers } from "./gutter";
import { manangMarkdownHighlight, manangTheme, manangTypstHighlight } from "./theme";
import type { Diagnostic } from "./diagnostics";

export interface EditorCallbacks {
  /** Fired on every document change. */
  readonly onChange: (text: string) => void;
  /** Fired when an auto-fix rewrites the document. */
  readonly onFix: (text: string) => void;
}

/**
 * Loads a language layer on demand.
 *
 * The grammar and its highlight style arrive in the same chunk as each other.
 * That coupling matters: importing the grammar's tag table at module scope
 * would pull the Typst package — including its WebAssembly loader — into the
 * initial bundle and defeat the code split entirely.
 */
export async function loadLanguage(language: DocumentLanguage): Promise<Extension> {
  if (language === "markdown") {
    const { markdown } = await import("@codemirror/lang-markdown");
    return [markdown(), syntaxHighlighting(manangMarkdownHighlight)];
  }

  // Typst, and LaTeX until a dedicated grammar lands. LaTeX reuses the Typst
  // grammar for the shared `$…$` math and markup; the buffer lives in Rust
  // either way, so this only affects editor highlighting.
  const { typst, typstHighlightTag } = await import("codemirror-lang-typst");
  return [typst(), syntaxHighlighting(manangTypstHighlight(typstHighlightTag))];
}

export function createEditor(
  parent: HTMLElement,
  initialText: string,
  language: DocumentLanguage,
  callbacks: EditorCallbacks,
  /** Extra extensions, e.g. the kernel sync listener. */
  extra: Extension = [],
): EditorView {
  const languageCompartment = new Compartment();

  const extensions: Extension[] = [
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

    keymap.of([
      ...closeBracketsKeymap,
      ...defaultKeymap,
      ...searchKeymap,
      ...historyKeymap,
      ...foldKeymap,
      ...completionKeymap,
      indentWithTab,
    ]),

    // Placeholder until the grammar chunk resolves.
    languageCompartment.of([]),
    manangTheme,
    whispers(callbacks.onFix),
    extra,

    EditorView.updateListener.of((update) => {
      if (!update.docChanged) return;
      callbacks.onChange(update.state.doc.toString());
    }),
  ];

  const view = new EditorView({
    parent,
    state: EditorState.create({ doc: initialText, extensions }),
  });

  // Upgrade in place once the grammar arrives. A failed load, or an editor that
  // was torn down while the chunk was in flight, is non-fatal: the editor stays
  // unhighlighted rather than refusing to open a document.
  void loadLanguage(language)
    .then((support) => {
      if (!view.dom.isConnected) return;
      view.dispatch({ effects: languageCompartment.reconfigure(support) });
    })
    .catch(() => {
      /* Editor remains usable without syntax support. */
    });

  return view;
}

/** Publishes diagnostics to a live editor. */
export function syncDiagnostics(
  view: EditorView,
  diagnostics: readonly Diagnostic[],
): void {
  pushDiagnostics(view, diagnostics);
}

/** Replaces the whole document, e.g. when another file is opened. */
export function replaceDocument(view: EditorView, text: string): void {
  view.dispatch({
    changes: { from: 0, to: view.state.doc.length, insert: text },
  });
}
