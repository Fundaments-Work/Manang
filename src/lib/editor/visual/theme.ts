/**
 * Styling of the visual layer, in the Fundaments.work system: square corners,
 * hairlines, tonal depth (no shadows), DM Sans at 300 and 400 and DM Mono at
 * 400, colours only from the app tokens. Bold is weight 400 against prose at
 * 300; the system has no heavier weight.
 */

import { EditorView } from "@codemirror/view";

export const visualTheme = EditorView.theme({
  "&.cm-visual .cm-content": {
    fontFamily: "var(--font-sans)",
    fontWeight: "300",
    fontVariantLigatures: "normal",
    padding: "16px 0 48px",
  },
  "&.cm-visual .cm-line": { padding: "0 24px" },

  // Headings: sized by level, DM Sans 300, markers hidden by the decorations.
  ".cm-vis-h": { fontFamily: "var(--font-sans)", fontWeight: "300", lineHeight: "1.3" },
  ".cm-vis-h1": { fontSize: "1.9em", paddingTop: "0.4em !important" },
  ".cm-vis-h2": { fontSize: "1.55em", paddingTop: "0.35em !important" },
  ".cm-vis-h3": { fontSize: "1.3em", paddingTop: "0.3em !important" },
  ".cm-vis-h4": { fontSize: "1.15em" },
  ".cm-vis-h5": { fontSize: "1.05em" },
  ".cm-vis-h6": { fontSize: "1em", color: "var(--mng-text-60)" },

  ".cm-vis-strong": { fontWeight: "400", color: "var(--mng-text)" },
  ".cm-vis-emph": { fontStyle: "italic" },
  ".cm-vis-strike": { textDecoration: "line-through" },
  ".cm-vis-code": {
    fontFamily: "var(--font-mono)",
    fontWeight: "400",
    fontSize: "0.92em",
    backgroundColor: "var(--mng-recessed)",
    padding: "0 3px",
  },
  ".cm-vis-link": {
    color: "var(--mng-math)",
    textDecoration: "underline",
    textUnderlineOffset: "2px",
  },
  ".cm-vis-ref": { color: "var(--mng-math)" },

  // Source that is shown on purpose (the caret is in its construct).
  ".cm-vis-marker": {
    fontFamily: "var(--font-mono)",
    fontWeight: "400",
    fontStyle: "normal",
    color: "var(--mng-text-60)",
  },
  ".cm-vis-source": { fontFamily: "var(--font-mono)", fontWeight: "400" },
  ".cm-vis-num": { color: "var(--mng-text-60)" },

  ".cm-vis-quote": {
    borderLeft: "1px solid var(--mng-text-60)",
    marginLeft: "24px",
    paddingLeft: "12px !important",
    color: "var(--mng-text-60)",
  },
  ".cm-vis-codeblock": {
    fontFamily: "var(--font-mono)",
    fontWeight: "400",
    fontSize: "0.92em",
    backgroundColor: "var(--mng-recessed)",
  },
  ".cm-vis-frontmatter-source": {
    fontFamily: "var(--font-mono)",
    fontWeight: "400",
    color: "var(--mng-text-60)",
    backgroundColor: "var(--mng-recessed)",
  },

  // Typst directives and anything the adapter leaves opaque: styled source.
  ".cm-vis-opaque": {
    fontFamily: "var(--font-mono)",
    fontWeight: "400",
    fontSize: "0.92em",
    color: "var(--mng-text-60)",
    backgroundColor: "var(--mng-recessed)",
  },
  ".cm-vis-comment": {
    fontFamily: "var(--font-mono)",
    fontStyle: "italic",
    color: "var(--mng-text-60)",
  },

  // Widgets.
  ".cm-vis-bullet": {
    display: "inline-block",
    width: "1.4em",
    textAlign: "center",
    color: "var(--mng-text-60)",
  },
  ".cm-vis-check": {
    display: "inline-block",
    width: "0.8em",
    height: "0.8em",
    margin: "0 0.6em 0 0.3em",
    border: "1px solid var(--mng-text-60)",
    verticalAlign: "-0.05em",
  },
  ".cm-vis-check-done": { backgroundColor: "var(--mng-text-60)" },
  ".cm-vis-rule": {
    display: "inline-block",
    width: "100%",
    borderTop: "1px solid var(--mng-line)",
    verticalAlign: "middle",
  },
  ".cm-vis-frontmatter": {
    display: "inline-block",
    fontFamily: "var(--font-mono)",
    fontSize: "12px",
    color: "var(--mng-text-60)",
    border: "1px solid var(--mng-line)",
    padding: "0 8px",
  },
  ".cm-vis-math": { display: "inline-block", verticalAlign: "middle" },
  ".cm-vis-math-display": {
    display: "block",
    textAlign: "center",
    margin: "0.4em 0",
  },
  ".cm-vis-math-img": { verticalAlign: "middle", maxWidth: "100%" },
  ".cm-vis-math.cm-vis-pending": {
    fontFamily: "var(--font-mono)",
    color: "var(--mng-text-60)",
  },
  ".cm-vis-math-error": {
    fontFamily: "var(--font-mono)",
    color: "var(--mng-text-60)",
    textDecoration: "underline wavy",
  },
  ".cm-vis-image": {
    display: "inline-block",
    verticalAlign: "middle",
    maxWidth: "100%",
  },
  ".cm-vis-image-figure": { display: "block", textAlign: "center", margin: "0.5em 0" },
  ".cm-vis-image-img": {
    maxWidth: "100%",
    maxHeight: "360px",
    backgroundColor: "var(--mng-paper)",
    border: "1px solid var(--mng-line)",
    verticalAlign: "middle",
    cursor: "default",
  },
  ".cm-vis-chip": {
    display: "inline-block",
    fontFamily: "var(--font-mono)",
    fontSize: "12px",
    color: "var(--mng-text-60)",
    border: "1px solid var(--mng-line)",
    padding: "0 8px",
  },
});
