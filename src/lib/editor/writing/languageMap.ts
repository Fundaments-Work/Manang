/**
 * Maps the editor's file languages onto the syntaxes the writing plugin
 * understands. Markup the plugin cannot parse (BibTeX, plain files) is checked
 * as plain text, so spelling still works and markup-aware rules stay quiet.
 */

import type { WritingSyntax } from "$lib/kernel/api";

export function writingSyntaxFor(language: string | null | undefined): WritingSyntax {
  switch (language) {
    case "typst":
    case "latex":
    case "markdown":
      return language;
    default:
      return "plain";
  }
}
