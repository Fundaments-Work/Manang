/**
 * In-memory `visual` plugin (ADR 0015) for the browser.
 *
 * It answers the same commands with the same shapes as
 * `crates/manang-visual-plugin`: sessions that mirror the editor document,
 * `visual.edit` returning a revision and a dirty range, `visual.blocks`
 * returning the **binary** span table (encoded with the contract's encoder, so
 * the web decoder is exercised exactly as against Rust), and edit plans for
 * the toolbar commands. Recognition is a small hand-written subset
 * (`mockVisualSpans.ts`); the real parsers are the Markdown and Typst adapters.
 * Keep the command list in step with `crates/manang-visual-plugin/manifest.toml`.
 */

import { SPAN_KIND, type EditPlan, type TextChange } from "$lib/editor/visual/protocol";
import { encodeSpanTable } from "$lib/editor/visual/table";

import type { MockPluginFactory } from "./mock";
import { toTable, spansOf, type MockFormat, type RawSpan } from "./mockVisualSpans";

const COMMANDS: readonly (readonly [string, string])[] = [
  ["visual.adapters", "List the formats the visual editor understands"],
  ["visual.open", "Open a visual session on a document"],
  ["visual.reset", "Resynchronise a visual session with its document"],
  ["visual.close", "Close a visual session"],
  ["visual.edit", "Apply text changes to a visual session"],
  ["visual.blocks", "Read the span table of a viewport"],
  ["visual.outline", "Read the headings of a visual session"],
  ["visual.command-state", "Read which formatting commands are enabled and active"],
  ["visual.run-command", "Plan the edits of a formatting command"],
  ["visual.bold", "Bold"],
  ["visual.italic", "Italic"],
  ["visual.code", "Code"],
  ["visual.strike", "Strikethrough"],
  ["visual.heading", "Heading"],
  ["visual.list", "List"],
  ["visual.quote", "Quote"],
  ["visual.link", "Link"],
  ["visual.image", "Image"],
  ["visual.figure", "Figure"],
  ["visual.math", "Formula"],
  ["visual.rule", "Horizontal rule"],
];

const ADAPTERS: Readonly<
  Record<
    MockFormat,
    {
      title: string;
      extensions: string[];
      kinds: string[];
      commands: string[];
    }
  >
> = {
  markdown: {
    title: "Markdown",
    extensions: ["md", "markdown"],
    kinds: [
      "heading",
      "strong",
      "emph",
      "code",
      "code-block",
      "list-item",
      "link",
      "image",
      "quote",
      "math",
      "frontmatter",
      "rule",
      "strike",
    ],
    commands: [
      "bold",
      "italic",
      "code",
      "strike",
      "heading",
      "list",
      "quote",
      "link",
      "image",
      "figure",
      "math",
      "rule",
    ],
  },
  typst: {
    title: "Typst",
    extensions: ["typ"],
    kinds: [
      "heading",
      "strong",
      "emph",
      "code",
      "list-item",
      "link",
      "image",
      "math",
      "ref",
      "label",
      "comment",
      "opaque",
    ],
    commands: [
      "bold",
      "italic",
      "code",
      "heading",
      "list",
      "link",
      "image",
      "figure",
      "math",
    ],
  },
};

interface Session {
  readonly format: MockFormat;
  text: string;
  revision: number;
}

const fail = (code: string, message: string): { message: string } => ({
  message: `[${code}] ${message}`,
});

function int(args: Record<string, unknown>, key: string): number {
  const value = args[key];
  if (typeof value !== "number" || !Number.isInteger(value) || value < 0) {
    throw fail("visual.bad-args", `\`${key}\` must be an unsigned integer`);
  }
  return value;
}

function formatFor(args: Record<string, unknown>): MockFormat {
  const adapter = args["adapter"];
  if (typeof adapter === "string") {
    if (adapter === "markdown" || adapter === "typst") return adapter;
    throw fail("visual.no-adapter", `no adapter \`${adapter}\``);
  }
  const path = args["path"];
  const ext =
    typeof path === "string" ? (path.split(".").pop() ?? "").toLowerCase() : "";
  for (const [id, info] of Object.entries(ADAPTERS)) {
    if (info.extensions.includes(ext)) return id as MockFormat;
  }
  throw fail("visual.no-adapter", "no adapter for this document");
}

export const visualPlugin: MockPluginFactory = (_emit, host) => {
  const sessions = new Map<number, Session>();
  let next = 1;

  const session = (args: Record<string, unknown>): Session => {
    const found = sessions.get(int(args, "session"));
    if (found === undefined) throw fail("visual.no-session", "no such session");
    return found;
  };

  const textOf = (args: Record<string, unknown>): string => {
    const inline = args["text"];
    if (typeof inline === "string") return inline;
    if (typeof args["doc"] === "number") {
      return host.call("editor.text", { id: args["doc"] }) as string;
    }
    throw fail("visual.bad-args", "`doc` or `text` is required");
  };

  return {
    name: "visual",
    commands: COMMANDS,
    handlers: {
      "visual.adapters": () =>
        Object.entries(ADAPTERS).map(([id, a]) => ({
          id,
          title: a.title,
          extensions: a.extensions,
          kinds: a.kinds,
          commands: a.commands,
        })),
      "visual.open": (args) => {
        const format = formatFor(args);
        const text = textOf(args);
        const id = next;
        next += 1;
        sessions.set(id, { format, text, revision: 0 });
        return { session: id, adapter: format, revision: 0, len: text.length };
      },
      "visual.reset": (args) => {
        const s = session(args);
        s.text = textOf(args);
        s.revision += 1;
        return {
          session: int(args, "session"),
          revision: s.revision,
          len: s.text.length,
        };
      },
      "visual.close": (args) => {
        sessions.delete(int(args, "session"));
        return null;
      },
      "visual.edit": (args) => {
        const s = session(args);
        const changes = args["changes"];
        if (!Array.isArray(changes))
          throw fail("visual.bad-args", "`changes` must be a list");
        const list = changes as TextChange[];
        let text = s.text;
        let previous = 0;
        for (const c of list) {
          if (c.from < previous || c.to < c.from || c.to > s.text.length) {
            throw fail(
              "visual.bad-args",
              "changes must be sorted, disjoint and in range",
            );
          }
          previous = c.to;
        }
        let delta = 0;
        let dirty: { from: number; to: number } | null = null;
        for (const c of [...list].reverse()) {
          text = text.slice(0, c.from) + c.insert + text.slice(c.to);
        }
        for (const c of list) {
          const from = c.from + delta;
          const to = from + c.insert.length;
          delta += c.insert.length - (c.to - c.from);
          dirty = dirty === null ? { from, to } : { from: dirty.from, to };
        }
        if (int(args, "len") !== text.length) {
          throw fail(
            "visual.desync",
            `the document has ${text.length} units, the caller says ${int(args, "len")}`,
          );
        }
        s.text = text;
        s.revision += 1;
        return {
          revision: s.revision,
          len: text.length,
          dirty:
            dirty === null
              ? null
              : {
                  from: Math.max(0, dirty.from - 1),
                  to: Math.min(text.length, dirty.to + 1),
                },
        };
      },
      "visual.outline": (args) => {
        const s = session(args);
        return spansOf(s.format, s.text)
          .filter((span) => span.kind === SPAN_KIND.heading)
          .map((span) => ({
            level: span.aux ?? 1,
            title: s.text
              .slice(span.start, span.end)
              .replace(/^[#=]+\s*/, "")
              .trim(),
            from: span.start,
            to: span.end,
          }));
      },
      "visual.command-state": (args) => {
        const s = session(args);
        const from = int(args, "from");
        const to = int(args, "to");
        const spans = spansOf(s.format, s.text);
        const has = (kind: number): boolean =>
          spans.some((sp) => sp.kind === kind && sp.start <= from && to <= sp.end);
        const active: Record<string, boolean> = {
          bold: has(SPAN_KIND.strong),
          italic: has(SPAN_KIND.emph),
          code: has(SPAN_KIND.code),
          strike: has(SPAN_KIND.strike),
          heading: has(SPAN_KIND.heading),
          list: has(SPAN_KIND.listItem),
          quote: has(SPAN_KIND.quote),
          link: has(SPAN_KIND.link),
        };
        return ADAPTERS[s.format].commands.map((command) => ({
          command,
          enabled: true,
          active: active[command] ?? false,
        }));
      },
      "visual.run-command": (args) => {
        const s = session(args);
        const command = args["command"];
        if (typeof command !== "string")
          throw fail("visual.bad-args", "`command` is required");
        if (!ADAPTERS[s.format].commands.includes(command)) {
          throw fail("visual.unsupported", `\`${command}\` is not supported here`);
        }
        const rest = args["args"];
        return plan(
          s,
          command,
          int(args, "from"),
          int(args, "to"),
          typeof rest === "string" ? rest : undefined,
        );
      },
    },
    binaryHandlers: {
      "visual.blocks": (args) => {
        const s = session(args);
        const len = s.text.length;
        const from = Math.min(int(args, "from"), len);
        const to = Math.min(Math.max(int(args, "to"), from), len);
        const reach = to === from ? from + 1 : to;
        const table = toTable(spansOf(s.format, s.text), len).filter(
          (span) => span.start < reach && span.end > from,
        );
        return encodeSpanTable({
          revision: s.revision,
          from,
          to,
          docLen: len,
          spans: table,
        });
      },
    },
  };
};

/* -------------------------------------------------------------------------- */
/* Edit plans                                                                 */
/* -------------------------------------------------------------------------- */

const INLINE: Readonly<Record<MockFormat, Readonly<Record<string, [string, number]>>>> =
  {
    markdown: {
      bold: ["**", SPAN_KIND.strong],
      italic: ["*", SPAN_KIND.emph],
      code: ["`", SPAN_KIND.code],
      strike: ["~~", SPAN_KIND.strike],
    },
    typst: {
      bold: ["*", SPAN_KIND.strong],
      italic: ["_", SPAN_KIND.emph],
      code: ["`", SPAN_KIND.code],
    },
  };

function lineBounds(
  text: string,
  from: number,
  to: number,
): { start: number; end: number } {
  const start = text.lastIndexOf("\n", Math.max(0, from) - 1) + 1;
  const stop = text.indexOf("\n", to);
  return { start, end: stop === -1 ? text.length : stop };
}

/** The edit plan of one command; exported for tests. */
export function plan(
  s: { format: MockFormat; text: string },
  command: string,
  from: number,
  to: number,
  arg?: string,
): EditPlan {
  const { format, text } = s;
  const typst = format === "typst";
  const inlineMarks = INLINE[format][command];
  if (inlineMarks !== undefined) {
    const [mark, kind] = inlineMarks;
    return toggleInline(spansOf(format, text), text, from, to, mark, kind);
  }
  switch (command) {
    case "heading": {
      const level = Math.max(0, Math.min(6, Number.parseInt(arg ?? "0", 10) || 0));
      const prefix = level === 0 ? "" : `${typst ? "=" : "#"}`.repeat(level) + " ";
      const changes: TextChange[] = [];
      const { start, end } = lineBounds(text, from, to);
      let at = start;
      while (at <= end) {
        const lineEnd = text.indexOf("\n", at);
        const stop = lineEnd === -1 || lineEnd > end ? end : lineEnd;
        const line = text.slice(at, stop);
        const old = (typst ? /^=+[ \t]+/ : /^#{1,6}[ \t]+/).exec(line)?.[0] ?? "";
        if (line.trim() !== "" && old !== prefix) {
          changes.push({ from: at, to: at + old.length, insert: prefix });
        }
        if (stop >= end) break;
        at = stop + 1;
      }
      return { changes, selection: null };
    }
    case "list":
    case "quote":
      return prefixLines(text, from, to, command, arg, typst);
    case "link": {
      const url = arg === undefined || arg === "" ? "https://" : arg;
      const label = text.slice(from, to) || "link";
      const open = typst ? `#link("${url}")[` : "[";
      const insert = typst ? `${open}${label}]` : `[${label}](${url})`;
      return {
        changes: [{ from, to, insert }],
        selection: { from: from + open.length, to: from + open.length + label.length },
      };
    }
    case "image":
    case "figure": {
      const path = arg ?? "";
      const insert = typst
        ? command === "figure"
          ? `#figure(image("${path}"), caption: [])`
          : `#image("${path}")`
        : `![](${path})`;
      const caret = from + insert.length - (typst && command === "figure" ? 1 : 0);
      return { changes: [{ from, to, insert }], selection: { from: caret, to: caret } };
    }
    case "math": {
      const body = text.slice(from, to) || "x";
      const block = arg === "block";
      const [open, close] = typst
        ? block
          ? ["$ ", " $"]
          : ["$", "$"]
        : block
          ? ["\n$$\n", "\n$$\n"]
          : ["$", "$"];
      return {
        changes: [{ from, to, insert: `${open}${body}${close}` }],
        selection: { from: from + open.length, to: from + open.length + body.length },
      };
    }
    case "rule": {
      const { end } = lineBounds(text, from, to);
      const empty = text.slice(lineBounds(text, from, to).start, end).trim() === "";
      const insert = empty ? "---\n" : "\n\n---\n";
      return {
        changes: [{ from: end, to: end, insert }],
        selection: { from: end + insert.length, to: end + insert.length },
      };
    }
    default:
      throw fail("visual.unsupported", `\`${command}\` is not supported here`);
  }
}

function toggleInline(
  spans: readonly RawSpan[],
  text: string,
  from: number,
  to: number,
  mark: string,
  kind: number,
): EditPlan {
  const w = mark.length;
  let enclosing: RawSpan | undefined;
  for (const span of spans) {
    if (span.kind === kind && span.start <= from && to <= span.end) enclosing = span;
  }
  if (enclosing !== undefined) {
    const nf = Math.max(enclosing.start, from - w);
    return {
      changes: [
        { from: enclosing.start, to: enclosing.start + w, insert: "" },
        { from: enclosing.end - w, to: enclosing.end, insert: "" },
      ],
      selection: { from: nf, to: Math.max(nf, to - w) },
    };
  }
  let a = from;
  let b = to;
  if (a === b) {
    while (a > 0 && /[\p{L}\p{N}]/u.test(text.charAt(a - 1))) a -= 1;
    while (b < text.length && /[\p{L}\p{N}]/u.test(text.charAt(b))) b += 1;
  }
  const nothing = a === b;
  const caret = nothing ? from + w : from + w;
  return {
    changes: nothing
      ? [{ from: a, to: a, insert: mark + mark }]
      : [
          { from: a, to: a, insert: mark },
          { from: b, to: b, insert: mark },
        ],
    selection: nothing ? { from: caret, to: caret } : { from: from + w, to: to + w },
  };
}

function prefixLines(
  text: string,
  from: number,
  to: number,
  command: string,
  arg: string | undefined,
  typst: boolean,
): EditPlan {
  const ordered = arg === "ordered";
  const strip = command === "quote" ? /^> ?/ : /^([-*+]|\d+[.)])[ \t]+/;
  const { start, end } = lineBounds(text, from, to);
  const rows: { at: number; line: string }[] = [];
  let at = start;
  for (;;) {
    const nl = text.indexOf("\n", at);
    const stop = nl === -1 || nl > end ? end : nl;
    rows.push({ at, line: text.slice(at, stop) });
    if (stop >= end) break;
    at = stop + 1;
  }
  const body = rows.filter((r) => r.line.trim() !== "");
  const kindOk = (line: string): boolean =>
    command === "quote"
      ? strip.test(line)
      : ordered
        ? /^(\d+[.)]|\+)[ \t]+/.test(line)
        : /^[-*][ \t]+/.test(line);
  const all = body.length > 0 && body.every((r) => kindOk(r.line));
  const changes: TextChange[] = [];
  let n = 1;
  for (const r of body) {
    const old =
      (command === "list" && typst ? /^([-+]|\d+[.)])[ \t]+/ : strip).exec(
        r.line,
      )?.[0] ?? "";
    const prefix = all
      ? ""
      : command === "quote"
        ? "> "
        : ordered
          ? typst
            ? "+ "
            : `${n}. `
          : "- ";
    n += 1;
    if (old !== prefix)
      changes.push({ from: r.at, to: r.at + old.length, insert: prefix });
  }
  return { changes, selection: null };
}
