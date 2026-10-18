/**
 * Editor language selection and syntax highlighting extensions.
 *
 * Supports:
 *   - Typst (.typ): via codemirror-lang-typst
 *   - LaTeX (.tex, .sty, .cls, .dtx, .ltx): via @codemirror/legacy-modes/mode/stex
 *   - BibTeX (.bib, .biblatex): via custom StreamLanguage
 *   - Markdown (.md, .markdown): via @codemirror/lang-markdown
 *   - Mermaid (.mmd, .mermaid): via a light StreamLanguage (languagesMermaid.ts)
 *   - Plain text (default / fallback)
 */

import {
  StreamLanguage,
  syntaxHighlighting,
  type StreamParser,
} from "@codemirror/language";
import type { Extension } from "@codemirror/state";
import { tags } from "@lezer/highlight";

import { extension } from "$lib/workbench/paths";

import {
  manangBibtexHighlight,
  manangLatexHighlight,
  manangMarkdownHighlight,
  manangTypstHighlight,
} from "./theme";

export type EditorLanguage =
  | "typst"
  | "latex"
  | "bibtex"
  | "markdown"
  | "mermaid"
  | "plain";

/**
 * Determines the editor language by file path extension.
 */
export function languageFor(path: string): EditorLanguage {
  const ext = extension(path);
  switch (ext) {
    case "typ":
      return "typst";
    case "tex":
    case "sty":
    case "cls":
    case "dtx":
    case "ltx":
      return "latex";
    case "bib":
    case "biblatex":
      return "bibtex";
    case "md":
    case "markdown":
      return "markdown";
    case "mmd":
    case "mermaid":
      return "mermaid";
    default:
      return "plain";
  }
}

export interface BibtexParserState {
  stage:
    | "top"
    | "after_at"
    | "key"
    | "fields"
    | "value"
    | "quoted_string"
    | "braced_string"
    | "comment_brace";
  braceDepth: number;
  valueBraceDepth: number;
}

const bibtexTokenTable = {
  entryType: tags.keyword,
  citationKey: tags.labelName,
  fieldName: tags.propertyName,
  string: tags.string,
  number: tags.number,
  comment: tags.comment,
  bracket: tags.bracket,
  punctuation: tags.punctuation,
  operator: tags.operator,
  variable: tags.variableName,
};

/**
 * Custom StreamParser for BibTeX files.
 */
export const bibtexParser: StreamParser<BibtexParserState> = {
  name: "bibtex",
  startState: () => ({
    stage: "top",
    braceDepth: 0,
    valueBraceDepth: 0,
  }),
  tokenTable: bibtexTokenTable,
  token(stream, state) {
    if (stream.eatSpace()) return null;

    if (state.stage === "quoted_string") {
      while (!stream.eol()) {
        if (stream.eat("\\")) {
          stream.next();
        } else if (stream.eat('"')) {
          state.stage = "fields";
          return "string";
        } else {
          stream.next();
        }
      }
      return "string";
    }

    if (state.stage === "braced_string") {
      while (!stream.eol()) {
        if (stream.eat("\\")) {
          stream.next();
        } else if (stream.eat("{")) {
          state.valueBraceDepth += 1;
        } else if (stream.eat("}")) {
          state.valueBraceDepth -= 1;
          if (state.valueBraceDepth <= 0) {
            state.stage = "fields";
            return "string";
          }
        } else {
          stream.next();
        }
      }
      return "string";
    }

    if (state.stage === "comment_brace") {
      while (!stream.eol()) {
        if (stream.eat("{")) {
          state.braceDepth += 1;
        } else if (stream.eat("}")) {
          state.braceDepth -= 1;
          if (state.braceDepth <= 0) {
            state.stage = "top";
            return "comment";
          }
        } else {
          stream.next();
        }
      }
      return "comment";
    }

    // Line comments (% ...)
    if (stream.match(/^%[^\n]*/)) {
      return "comment";
    }

    if (state.stage === "top") {
      if (stream.eat("@")) {
        if (stream.match(/^[A-Za-z]+/i)) {
          const type = stream.current().slice(1).toLowerCase();
          if (type === "comment") {
            state.stage = "comment_brace";
            state.braceDepth = 0;
            return "comment";
          }
          state.stage = "after_at";
          return "entryType";
        }
        return "entryType";
      }
      // Any text outside entries in BibTeX is treated as comment text
      stream.match(/^[^@%]+/);
      return "comment";
    }

    if (state.stage === "after_at") {
      if (stream.eat("{") || stream.eat("(")) {
        state.braceDepth = 1;
        state.stage = "key";
        return "bracket";
      }
      stream.next();
      return null;
    }

    if (state.stage === "key") {
      if (stream.match(/^[^,\s{}()]+/)) {
        state.stage = "fields";
        return "citationKey";
      }
      if (stream.eat(",")) {
        state.stage = "fields";
        return "punctuation";
      }
      if (stream.eat("}") || stream.eat(")")) {
        state.stage = "top";
        return "bracket";
      }
      stream.next();
      return null;
    }

    if (state.stage === "fields") {
      if (stream.eat("}") || stream.eat(")")) {
        state.braceDepth -= 1;
        if (state.braceDepth <= 0) {
          state.stage = "top";
        }
        return "bracket";
      }
      if (stream.eat(",")) {
        return "punctuation";
      }
      if (stream.eat("=")) {
        state.stage = "value";
        return "operator";
      }
      if (stream.match(/^[A-Za-z][\w-]*/)) {
        return "fieldName";
      }
      stream.next();
      return null;
    }

    if (state.stage === "value") {
      if (stream.eat('"')) {
        state.stage = "quoted_string";
        return "string";
      }
      if (stream.eat("{")) {
        state.stage = "braced_string";
        state.valueBraceDepth = 1;
        return "string";
      }
      if (stream.match(/^\d+/)) {
        state.stage = "fields";
        return "number";
      }
      if (stream.match(/^[A-Za-z][\w-]*/)) {
        return "variable";
      }
      if (stream.eat("#")) {
        return "operator";
      }
      if (stream.eat(",")) {
        state.stage = "fields";
        return "punctuation";
      }
      if (stream.eat("}") || stream.eat(")")) {
        state.stage = "top";
        return "bracket";
      }
      stream.next();
      return null;
    }

    stream.next();
    return null;
  },
};

export const bibtexLanguage = StreamLanguage.define(bibtexParser);

/**
 * Loads a language extension on demand.
 */
export async function loadLanguageExtension(
  language: EditorLanguage,
): Promise<Extension> {
  switch (language) {
    case "plain":
      return [];
    case "markdown": {
      const { markdown } = await import("@codemirror/lang-markdown");
      return [markdown(), syntaxHighlighting(manangMarkdownHighlight)];
    }
    case "latex": {
      const { stex } = await import("@codemirror/legacy-modes/mode/stex");
      return [StreamLanguage.define(stex), syntaxHighlighting(manangLatexHighlight)];
    }
    case "bibtex":
      return [bibtexLanguage, syntaxHighlighting(manangBibtexHighlight)];
    case "mermaid": {
      // Same token colours as BibTeX: keywords, names, strings, comments.
      const { mermaidLanguage } = await import("./languagesMermaid");
      return [mermaidLanguage, syntaxHighlighting(manangBibtexHighlight)];
    }
    case "typst": {
      const { typst, typstHighlightTag } = await import("codemirror-lang-typst");
      return [typst(), syntaxHighlighting(manangTypstHighlight(typstHighlightTag))];
    }
  }
}
