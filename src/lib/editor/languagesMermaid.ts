/**
 * A light Mermaid mode: diagram keywords, arrows, `%%` comments, quoted
 * strings and bracketed node labels. Enough to read a diagram at a glance;
 * the `diagram` plugin is the parser that decides what is valid.
 */

import { StreamLanguage, type StreamParser } from "@codemirror/language";
import { tags } from "@lezer/highlight";

const KEYWORDS = new Set([
  "flowchart",
  "graph",
  "sequenceDiagram",
  "classDiagram",
  "stateDiagram",
  "stateDiagram-v2",
  "erDiagram",
  "gantt",
  "pie",
  "journey",
  "mindmap",
  "timeline",
  "gitGraph",
  "subgraph",
  "end",
  "direction",
  "participant",
  "actor",
  "loop",
  "alt",
  "else",
  "opt",
  "par",
  "and",
  "note",
  "over",
  "activate",
  "deactivate",
  "class",
  "classDef",
  "style",
  "linkStyle",
  "click",
  "title",
  "section",
  "TD",
  "TB",
  "BT",
  "LR",
  "RL",
]);

/** Which closing bracket ends the node label being read, if any. */
export interface MermaidState {
  label: string | null;
}

const CLOSE: Readonly<Record<string, string>> = { "[": "]", "(": ")", "{": "}" };

const tokenTable = {
  keyword: tags.keyword,
  operator: tags.operator,
  comment: tags.comment,
  string: tags.string,
  variable: tags.variableName,
  bracket: tags.bracket,
};

export const mermaidParser: StreamParser<MermaidState> = {
  name: "mermaid",
  startState: () => ({ label: null }),
  tokenTable,
  token(stream, state) {
    if (state.label !== null) {
      const close = state.label;
      while (!stream.eol()) {
        if (stream.peek() === close) {
          if (stream.current() === "") {
            stream.next();
            state.label = null;
            return "bracket";
          }
          return "string";
        }
        stream.next();
      }
      return "string";
    }
    if (stream.eatSpace()) return null;
    if (stream.match(/^%%.*/)) return "comment";
    if (stream.match(/^"[^"]*"?/)) return "string";
    // Arrows and links: -->, ---, -.->, ==>, ->>, -->>, --x, <-->, -) ...
    if (stream.match(/^<?(?:[-=.]{2,}[>xo]{0,2}|->>?|-[x)])/)) return "operator";
    if (stream.match(/^\|[^|]*\|/)) return "string";
    const open = stream.peek();
    if (open !== undefined && CLOSE[open] !== undefined) {
      stream.next();
      // `[[`, `((`, `{{` shapes: the doubled bracket is one token.
      if (stream.peek() === open) stream.next();
      state.label = CLOSE[open] ?? null;
      return "bracket";
    }
    if (stream.match(/^[A-Za-z_]\w*(?:-\w+)*/)) {
      return KEYWORDS.has(stream.current()) ? "keyword" : "variable";
    }
    stream.next();
    return null;
  },
};

export const mermaidLanguage = StreamLanguage.define(mermaidParser);
