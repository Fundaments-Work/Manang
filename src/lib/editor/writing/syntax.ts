/**
 * Which markup language a document is written in, as the writing plugin names it.
 *
 * The editor state does not carry the file path, but it does carry the language
 * layer that `view.ts` loads lazily: `markdown` and `typst` (LaTeX reuses the
 * Typst grammar for highlighting, so it is told apart by a look at the start of
 * the text). Before the layer arrives the answer is "auto", which the Rust
 * plugin resolves itself.
 */

import { language } from "@codemirror/language";
import type { EditorState } from "@codemirror/state";

import type { WritingSyntax } from "$lib/kernel/api";

const LATEX_MARKERS =
  /\\(documentclass|usepackage|begin\{|section\{|chapter\{|title\{)/;

/** How much of the document head is sniffed to tell LaTeX from Typst. */
const SNIFF = 4096;

type Resolver = (state: EditorState) => WritingSyntax | null;
let resolver: Resolver | null = null;

/**
 * The app knows the shown file's language from its path, which is more exact
 * than anything the state carries (`.tex` files share the Typst grammar). It
 * registers that knowledge here; without it the state is inspected.
 */
export function setSyntaxResolver(next: Resolver | null): void {
  resolver = next;
}

export function writingSyntax(state: EditorState): WritingSyntax {
  const known = resolver?.(state) ?? null;
  if (known !== null && known !== "plain") return known;
  const name = state.facet(language)?.name ?? "";
  if (name === "markdown") return "markdown";
  if (name.includes("tex")) return "latex";
  if (name === "typst") {
    const head = state.doc.sliceString(0, Math.min(state.doc.length, SNIFF));
    return LATEX_MARKERS.test(head) ? "latex" : "typst";
  }
  return "auto";
}

/** The three syntaxes that can be formatted or expanded; `null` for anything else. */
export function editingSyntax(
  syntax: WritingSyntax,
): "typst" | "latex" | "markdown" | null {
  return syntax === "typst" || syntax === "latex" || syntax === "markdown"
    ? syntax
    : null;
}
