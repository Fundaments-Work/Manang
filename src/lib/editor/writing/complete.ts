/**
 * Autocomplete sources from the writing plugin: math symbols (inserted in the
 * document's own syntax, with the Unicode glyph as a preview) and snippets with
 * tab stops. Both are asked for lazily, cached, and never block typing: a source
 * that has not answered yet simply offers nothing.
 */

import {
  snippet,
  type Completion,
  type CompletionContext,
  type CompletionResult,
  type CompletionSource,
} from "@codemirror/autocomplete";
import { EditorState, type Extension } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";

import type {
  SnippetExpansion,
  SnippetInfo,
  SymbolHit,
  WritingApi,
  WritingSyntax,
} from "$lib/kernel/api";

import { Lru } from "./chunks";
import { editingSyntax, writingSyntax } from "./syntax";

/* -------------------------------------------------------------------------- */
/* Snippet templates                                                           */
/* -------------------------------------------------------------------------- */

/**
 * CodeMirror's snippet grammar only treats a backslash as an escape before a
 * brace, and this escapes every brace. A literal backslash must stay as-is;
 * escaping it too would add a second backslash to the inserted text.
 */
const escapeBraces = (text: string): string => text.replace(/[{}]/g, "\\$&");

/**
 * Turns the plugin's expansion (plain text plus scalar-offset tab stops) into a
 * CodeMirror snippet template (`${1:name}`, `${0}`), so CodeMirror's own Tab and
 * Shift+Tab navigation, mirrored fields and indentation do the work.
 */
export function snippetTemplate(expansion: SnippetExpansion): string {
  const points = Array.from(expansion.text);
  interface Field {
    start: number;
    end: number;
    index: number;
  }
  const fields: Field[] = [];
  for (const stop of expansion.tab_stops) {
    for (const range of stop.ranges) {
      fields.push({ start: range.start, end: range.end, index: stop.index });
    }
  }
  fields.push({
    start: expansion.final_cursor,
    end: expansion.final_cursor,
    index: 0,
  });
  fields.sort((a, b) => a.start - b.start || a.end - b.end);

  let out = "";
  let at = 0;
  for (const field of fields) {
    if (field.start < at || field.end > points.length) continue;
    out += escapeBraces(points.slice(at, field.start).join(""));
    const content = points.slice(field.start, field.end).join("");
    if (/[{}\n]/.test(content)) {
      // CodeMirror cannot carry braces or line breaks in a placeholder.
      out += escapeBraces(content);
    } else {
      out += content === "" ? `\${${field.index}}` : `\${${field.index}:${content}}`;
    }
    at = field.end;
  }
  out += escapeBraces(points.slice(at).join(""));
  return out;
}

/* -------------------------------------------------------------------------- */
/* Symbols                                                                     */
/* -------------------------------------------------------------------------- */

type MathSyntax = "typst" | "latex";

function mathSyntaxFor(syntax: WritingSyntax): MathSyntax | null {
  if (syntax === "typst") return "typst";
  // Markdown math is TeX: `$\alpha$`.
  if (syntax === "latex" || syntax === "markdown") return "latex";
  return null;
}

/** True when the caret sits between an odd number of unescaped `$` in its paragraph. */
export function inMath(state: EditorState, pos: number): boolean {
  const before = state.doc.sliceString(Math.max(0, pos - 2000), pos);
  const paragraph = before.split(/\n[ \t]*\n/).pop() ?? "";
  return ((paragraph.match(/(?<!\\)\$/g) ?? []).length & 1) === 1;
}

/** The query being typed, or null when symbols should stay out of the way. */
export function symbolQuery(
  state: EditorState,
  pos: number,
  syntax: MathSyntax,
  explicit: boolean,
): { from: number; query: string } | null {
  const line = state.doc.lineAt(pos);
  const text = line.text.slice(0, pos - line.from);
  if (syntax === "latex") {
    const match = /\\[A-Za-z]*$/.exec(text);
    if (match === null) return null;
    return { from: pos - match[0].length, query: match[0].slice(1) };
  }
  const match = /[A-Za-z][A-Za-z.]*$/.exec(text);
  if (match === null) return null;
  if (!inMath(state, pos)) return null;
  if (match[0].length < 2 && !explicit) return null;
  return { from: pos - match[0].length, query: match[0] };
}

function symbolCompletion(hit: SymbolHit, syntax: MathSyntax): Completion {
  const label = syntax === "latex" ? hit.insert : hit.name;
  return {
    label,
    detail: hit.preview,
    info:
      hit.unicode === null
        ? hit.category
        : `${hit.preview}  ${hit.unicode}${hit.requires === null ? "" : `, needs ${hit.requires}`}`,
    apply: hit.insert,
    type: "constant",
    boost: hit.score,
  };
}

export function symbolSource(
  api: WritingApi,
  syntaxOf: (state: EditorState) => WritingSyntax = writingSyntax,
): CompletionSource {
  const cache = new Lru<readonly Completion[]>(60);
  return async (context: CompletionContext): Promise<CompletionResult | null> => {
    const syntax = mathSyntaxFor(syntaxOf(context.state));
    if (syntax === null) return null;
    const target = symbolQuery(context.state, context.pos, syntax, context.explicit);
    if (target === null) return null;

    const key = `${syntax}|${target.query}`;
    let options = cache.get(key);
    if (options === undefined) {
      try {
        const { symbols } = await api.symbols(target.query, syntax, 20);
        options = symbols.map((hit) => symbolCompletion(hit, syntax));
      } catch {
        return null;
      }
      cache.set(key, options);
    }
    if (context.aborted || options.length === 0) return null;
    // The server already ranked them; CodeMirror must not re-filter by label.
    return { from: target.from, options: [...options], filter: false };
  };
}

/* -------------------------------------------------------------------------- */
/* Snippets                                                                    */
/* -------------------------------------------------------------------------- */

const SNIPPET_TTL_MS = 60_000;

export function snippetSource(
  api: WritingApi,
  syntaxOf: (state: EditorState) => WritingSyntax = writingSyntax,
): CompletionSource {
  const lists = new Map<
    string,
    { at: number; list: Promise<readonly SnippetInfo[]> }
  >();

  const listFor = (
    syntax: "typst" | "latex" | "markdown",
  ): Promise<readonly SnippetInfo[]> => {
    const held = lists.get(syntax);
    if (held !== undefined && Date.now() - held.at < SNIPPET_TTL_MS) return held.list;
    const list = api.snippets(syntax).then(
      (r) => r.snippets,
      () => [] as SnippetInfo[],
    );
    lists.set(syntax, { at: Date.now(), list });
    return list;
  };

  return async (context: CompletionContext): Promise<CompletionResult | null> => {
    const syntax = editingSyntax(syntaxOf(context.state));
    if (syntax === null) return null;
    const word = context.matchBefore(/[A-Za-z][\w-]*/);
    if (word === null || (word.to - word.from < 2 && !context.explicit)) return null;

    const snippets = await listFor(syntax);
    if (context.aborted) return null;

    const options: Completion[] = [];
    for (const item of snippets) {
      for (const trigger of [item.trigger, ...item.aliases]) {
        options.push({
          label: trigger,
          detail: item.name,
          info: item.description,
          type: "text",
          apply: (
            view: EditorView,
            _completion: Completion,
            from: number,
            to: number,
          ) => void expandInto(api, view, item.trigger, syntax, from, to),
        });
      }
    }
    return { from: word.from, options, validFor: /^[\w-]*$/ };
  };
}

async function expandInto(
  api: WritingApi,
  view: EditorView,
  trigger: string,
  syntax: "typst" | "latex" | "markdown",
  from: number,
  to: number,
): Promise<void> {
  const typed = view.state.sliceDoc(from, to);
  try {
    const expansion = await api.expand(trigger, syntax);
    // Typing continued while the plugin answered: leave the text alone.
    if (view.state.sliceDoc(from, to) !== typed) return;
    snippet(snippetTemplate(expansion))(view, null, from, to);
  } catch {
    /* The typed trigger stays as it is. */
  }
}

/** Both sources, registered for every language the editor shows. */
export function writingCompletions(api: WritingApi): Extension {
  const symbols = symbolSource(api);
  const snippets = snippetSource(api);
  return EditorState.languageData.of(() => [
    { autocomplete: symbols },
    { autocomplete: snippets },
  ]);
}
