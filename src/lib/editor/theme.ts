/**
 * CodeMirror 6 theme, bound to the design tokens in `app.css`.
 *
 * Every colour is a `var()` reference rather than a literal, so the editor
 * follows the runtime light/dark override automatically and can never drift
 * from the rest of the shell. That is the same discipline as the paper tokens:
 * one source of colour truth.
 *
 * The theme itself imports nothing from any grammar, so it can be part of the
 * initial bundle. Only the *highlight style* depends on grammar tags, and that
 * is built inside the lazy language chunk (see `view.ts`).
 */

import {
  HighlightStyle,
  type HighlightStyle as HighlightStyleType,
} from "@codemirror/language";
import { EditorView } from "@codemirror/view";
import { tags, type Tag } from "@lezer/highlight";

export const manangTheme = EditorView.theme({
  "&": {
    height: "100%",
    backgroundColor: "var(--mng-bg)",
    color: "var(--mng-text)",
    // Set from the editor.font-size and editor.line-height settings.
    fontSize: "var(--mng-editor-font-size, 14px)",
  },

  ".cm-content": {
    fontFamily: "var(--font-mono)",
    fontWeight: "400",
    // Source code: `fi` and `ff` stay two letters, so columns and carets line up.
    fontVariantLigatures: "none",
    lineHeight: "var(--mng-editor-line-height, 1.6)",
    caretColor: "var(--mng-accent)",
    padding: "12px 0",
  },

  ".cm-scroller": {
    fontFamily: "var(--font-mono)",
    // The system forbids rounded corners, including on scrollbars.
    scrollbarColor: "var(--mng-line) transparent",
  },

  // Line numbers are data, so they are set in the mono face and never compete
  // with the manuscript for attention.
  ".cm-gutters": {
    backgroundColor: "var(--mng-bg)",
    color: "var(--mng-text-muted)",
    border: "none",
    paddingRight: "4px",
  },

  ".cm-lineNumbers .cm-gutterElement": {
    padding: "0 8px 0 16px",
    minWidth: "44px",
  },

  ".cm-activeLine": { backgroundColor: "transparent" },
  ".cm-activeLineGutter": {
    backgroundColor: "transparent",
    color: "var(--mng-text-60)",
  },

  ".cm-cursor, .cm-dropCursor": {
    borderLeftColor: "var(--mng-accent)",
    borderLeftWidth: "1px",
  },

  "&.cm-focused": { outline: "none" },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground": {
    backgroundColor: "color-mix(in srgb, var(--mng-accent) 32%, transparent)",
  },
  "&.cm-focused .cm-matchingBracket": {
    backgroundColor: "color-mix(in srgb, var(--mng-accent) 20%, transparent)",
    color: "inherit",
    outline: "none",
  },

  ".cm-selectionMatch": { backgroundColor: "transparent" },

  ".cm-searchMatch": {
    backgroundColor: "color-mix(in srgb, var(--mng-math) 22%, transparent)",
    outline: "1px solid color-mix(in srgb, var(--mng-math) 40%, transparent)",
  },
  ".cm-searchMatch.cm-searchMatch-selected": {
    backgroundColor: "color-mix(in srgb, var(--mng-accent) 28%, transparent)",
  },

  ".cm-panels": {
    backgroundColor: "var(--mng-surface)",
    color: "var(--mng-text)",
    borderTop: "1px solid var(--mng-line)",
  },
  ".cm-panel.cm-search input, .cm-panel.cm-search button": {
    fontFamily: "var(--font-sans)",
    fontSize: "12px",
    color: "var(--mng-text)",
    backgroundColor: "var(--mng-surface-hi)",
    border: "1px solid var(--mng-line)",
    borderRadius: "0",
    padding: "2px 6px",
  },

  ".cm-tooltip": {
    backgroundColor: "var(--mng-surface)",
    border: "1px solid var(--mng-line)",
    borderRadius: "0",
    color: "var(--mng-text)",
    fontFamily: "var(--font-sans)",
    fontSize: "12px",
  },
  ".cm-tooltip-autocomplete > ul > li": {
    padding: "2px 6px",
    fontFamily: "var(--font-mono)",
  },
  ".cm-tooltip-autocomplete > ul > li[aria-selected]": {
    backgroundColor: "color-mix(in srgb, var(--mng-accent) 28%, transparent)",
    color: "var(--mng-text)",
  },
});

/**
 * Builds the Typst highlight style from the grammar's own tags.
 *
 * The grammar publishes 22 semantic tags as an open `Record<string, Tag>`.
 * Under `noUncheckedIndexedAccess` every lookup is `Tag | undefined`, so tags
 * resolve through this accessor: a renamed upstream tag then fails loudly
 * instead of silently dropping all highlighting.
 *
 * Mapping follows DESIGN.md §3.2 "Quiet Syntax Highlighting":
 *   prose              text primary
 *   math delimiters    the maths token (sage/lime)
 *   headings           DM Sans 300, warm ivory
 *   functions, keys    lime
 *   comments           low-contrast, italic
 */
export function manangTypstHighlight(
  tags: Readonly<Record<string, Tag>>,
): HighlightStyleType {
  function tag(name: string): Tag {
    const found = tags[name];
    if (found === undefined) {
      throw new Error(`language grammar: unknown tag "${name}"`);
    }
    return found;
  }

  return HighlightStyle.define([
    // Headings are set in the sans face at weight 300, which is the only way the
    // system distinguishes them from prose without a bolder weight.
    {
      tag: tag("typ-heading"),
      color: "var(--mng-text)",
      fontFamily: "var(--font-sans)",
      fontWeight: "300",
    },

    {
      tag: [tag("typ-math-delim"), tag("typ-math-op"), tag("typ-math-group")],
      color: "var(--mng-math)",
    },

    {
      tag: [
        tag("typ-func"),
        tag("typ-key"),
        tag("typ-ref"),
        tag("typ-label"),
        tag("typ-link"),
      ],
      color: "var(--mng-accent)",
    },

    {
      tag: tag("typ-comment"),
      color: "var(--mng-text-muted)",
      fontStyle: "italic",
    },

    {
      tag: [tag("typ-num"), tag("typ-str"), tag("typ-pol"), tag("typ-term")],
      color: "var(--mng-text-60)",
    },

    {
      tag: [tag("typ-marker"), tag("typ-punct"), tag("typ-op")],
      color: "var(--mng-text-60)",
    },

    {
      tag: [tag("typ-raw"), tag("typ-escape")],
      color: "var(--mng-text-60)",
    },

    // Two weights exist in the system, so emphasis is carried by italic and
    // colour alone. `typ-strong` cannot bold without breaking the type system,
    // so only `typ-emph` is distinguished.
    { tag: tag("typ-emph"), fontStyle: "italic" },

    // Parse errors are quiet, not an alarm: the gutter carries the signal
    // and the prose stays readable rather than flooding the line with colour.
    {
      tag: tag("typ-error"),
      color: "var(--mng-text-60)",
      textDecoration: "underline wavy",
    },
  ]);
}

/**
 * Markdown highlight style, from the standard Lezer tags.
 *
 * `lang-markdown` ships no default style, and building one here keeps the
 * palette identical to the Typst one: same four colours, same hierarchy, same
 * refusal to introduce a fifth hue.
 */
export const manangMarkdownHighlight = HighlightStyle.define([
  {
    tag: tags.heading,
    color: "var(--mng-text)",
    fontFamily: "var(--font-sans)",
    fontWeight: "300",
  },
  { tag: tags.link, color: "var(--mng-accent)" },
  { tag: tags.url, color: "var(--mng-accent)", textDecoration: "underline" },
  { tag: tags.emphasis, fontStyle: "italic" },
  { tag: tags.strong, color: "var(--mng-text)" },
  { tag: [tags.monospace, tags.labelName], color: "var(--mng-math)" },
  {
    tag: [tags.quote, tags.contentSeparator],
    color: "var(--mng-text-60)",
    fontStyle: "italic",
  },
  { tag: [tags.list, tags.processingInstruction], color: "var(--mng-text-60)" },
  { tag: tags.comment, color: "var(--mng-text-muted)", fontStyle: "italic" },
]);

/**
 * LaTeX highlight style for CodeMirror 6 stex stream parser.
 */
export const manangLatexHighlight = HighlightStyle.define([
  { tag: tags.keyword, color: "var(--mng-text)" },
  { tag: [tags.atom, tags.standard(tags.variableName)], color: "var(--mng-math)" },
  { tag: tags.comment, color: "var(--mng-text-muted)", fontStyle: "italic" },
  { tag: [tags.number, tags.string], color: "var(--mng-text-60)" },
  { tag: [tags.bracket, tags.punctuation, tags.operator], color: "var(--mng-text-60)" },
  { tag: tags.labelName, color: "var(--mng-math)" },
]);

/**
 * BibTeX highlight style for the BibTeX stream parser.
 */
export const manangBibtexHighlight = HighlightStyle.define([
  { tag: [tags.keyword, tags.typeName], color: "var(--mng-text)" },
  { tag: tags.labelName, color: "var(--mng-math)" },
  { tag: tags.propertyName, color: "var(--mng-text)" },
  { tag: tags.string, color: "var(--mng-text-60)" },
  { tag: tags.number, color: "var(--mng-text-60)" },
  { tag: tags.comment, color: "var(--mng-text-muted)", fontStyle: "italic" },
  { tag: [tags.bracket, tags.punctuation, tags.operator], color: "var(--mng-text-60)" },
  { tag: tags.variableName, color: "var(--mng-math)" },
]);
