/**
 * In-memory `markdown` plugin.
 *
 * A small CommonMark subset (headings, paragraphs, lists, code, emphasis, links,
 * block quotes, rules) that escapes everything else, so the Markdown preview can
 * be exercised without Rust. Result shapes mirror `crates/manang-markdown`
 * (`plugin.rs` `markdown.render-html`, `markdown.outline` and
 * `markdown.to-typst`, `outline.rs`, `typst.rs`).
 * The real renderer sanitises raw HTML; this one never emits any.
 */

import type { MockPlugin, MockPluginFactory } from "./mock";
import type {
  DiagramLanguageId,
  MarkdownDiagnostic,
  MarkdownFence,
  MarkdownHeading,
  MarkdownRender,
} from "./types";

const COMMANDS: readonly (readonly [string, string])[] = [
  ["markdown.render-html", "Render Markdown to sanitized HTML"],
  ["markdown.outline", "Extract Markdown document outline"],
  ["markdown.to-typst", "Convert Markdown to Typst markup"],
  ["markdown.diagrams", "List the diagram fences of a Markdown document"],
];

function escape(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function inline(text: string): string {
  return escape(text)
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/\*([^*]+)\*/g, "<em>$1</em>")
    .replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (_all, label: string, href: string) =>
      /^(https?:|mailto:|#|[\w./-]+$)/.test(href)
        ? `<a href="${href}">${label}</a>`
        : label,
    );
}

function slug(title: string): string {
  return title
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
}

/** Strip a leading `---` frontmatter block; returns the body and how many lines it took. */
function split(source: string): {
  body: string;
  skipped: number;
  frontmatter: MarkdownRender["frontmatter"];
} {
  const m = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?/.exec(source);
  if (m === null) return { body: source, skipped: 0, frontmatter: null };
  const data: Record<string, string> = {};
  for (const line of (m[1] ?? "").split("\n")) {
    const kv = /^([A-Za-z_][\w-]*):\s*(.*)$/.exec(line);
    if (kv?.[1] !== undefined) data[kv[1]] = (kv[2] ?? "").replace(/^["']|["']$/g, "");
  }
  return {
    body: source.slice(m[0].length),
    skipped: m[0].split("\n").length - 1,
    frontmatter: { format: "yaml", raw: m[1] ?? "", data },
  };
}

/** Fence info words and their diagram language, as `crates/manang-markdown` `diagrams.rs`. */
const FENCES: Readonly<Record<string, DiagramLanguageId>> = {
  mermaid: "mermaid",
  pikchr: "pikchr",
  bob: "svgbob",
  svgbob: "svgbob",
};

function diagramFence(info: string): DiagramLanguageId | null {
  const word = info.trim().split(/\s+/)[0]?.toLowerCase() ?? "";
  return FENCES[word] ?? null;
}

/** FNV-1a 64 over language, NUL, source: the Rust plugin's `fence_key`. */
export function fenceKey(language: string, source: string): string {
  let hash = 0xcbf29ce484222325n;
  const bytes = new TextEncoder().encode(`${language}\u0000${source}`);
  for (const byte of bytes) {
    hash ^= BigInt(byte);
    hash = (hash * 0x100000001b3n) & 0xffffffffffffffffn;
  }
  return hash.toString(16).padStart(16, "0");
}

function makeFence(
  language: DiagramLanguageId,
  code: readonly string[],
  line: number,
  endLine: number,
): MarkdownFence {
  const source = code.length === 0 ? "" : `${code.join("\n")}\n`;
  return { language, source, line, endLine, key: fenceKey(language, source) };
}

function placeholder(fence: MarkdownFence): string {
  return `<figure class="mng-diagram" data-diagram="${fence.key}" data-language="${fence.language}" data-line="${fence.line}"></figure>`;
}

export function render(source: string): MarkdownRender {
  const { body, skipped, frontmatter } = split(source);
  const headings: MarkdownHeading[] = [];
  const diagnostics: MarkdownDiagnostic[] = [];
  const html: string[] = [];
  let paragraph: string[] = [];
  let list: "ul" | "ol" | null = null;
  let fence = false;
  let code: string[] = [];
  let fenceLanguage: DiagramLanguageId | null = null;
  let fenceLine = 0;
  const diagrams: MarkdownFence[] = [];

  const flush = (): void => {
    if (paragraph.length > 0) html.push(`<p>${inline(paragraph.join(" "))}</p>`);
    paragraph = [];
  };
  const close = (): void => {
    if (list !== null) html.push(`</${list}>`);
    list = null;
  };

  const lines = body.split("\n");
  lines.forEach((raw, index) => {
    const line = raw.replace(/\r$/, "");
    const lineNo = index + 1 + skipped;
    if (line.startsWith("```")) {
      if (fence) {
        if (fenceLanguage !== null) {
          const fenced = makeFence(fenceLanguage, code, fenceLine, lineNo);
          diagrams.push(fenced);
          html.push(placeholder(fenced));
        } else {
          html.push(`<pre><code>${escape(code.join("\n"))}\n</code></pre>`);
        }
        code = [];
      } else {
        flush();
        close();
        fenceLanguage = diagramFence(line.slice(3));
        fenceLine = lineNo;
      }
      fence = !fence;
      return;
    }
    if (fence) {
      code.push(line);
      return;
    }
    if (/^\s*<\/?(script|iframe|style|object)\b/i.test(line)) {
      diagnostics.push({
        severity: "warning",
        message: "Raw HTML is not allowed and was removed",
        line: lineNo,
      });
      return;
    }
    const heading = /^(#{1,6})\s+(.*?)\s*#*$/.exec(line);
    if (heading !== null) {
      flush();
      close();
      const title = heading[2] ?? "";
      const level = (heading[1] ?? "#").length;
      headings.push({ title, level, line: lineNo });
      html.push(`<h${level} id="${slug(title)}">${inline(title)}</h${level}>`);
      return;
    }
    const item = /^\s*([-*+]|\d+\.)\s+(.*)$/.exec(line);
    if (item !== null) {
      flush();
      const kind = /\d/.test(item[1] ?? "") ? "ol" : "ul";
      if (list !== kind) {
        close();
        html.push(`<${kind}>`);
        list = kind;
      }
      html.push(`<li>${inline(item[2] ?? "")}</li>`);
      return;
    }
    if (/^\s*([-*_])\1\1+\s*$/.test(line)) {
      flush();
      close();
      html.push("<hr>");
      return;
    }
    const quote = /^>\s?(.*)$/.exec(line);
    if (quote !== null) {
      flush();
      close();
      html.push(`<blockquote><p>${inline(quote[1] ?? "")}</p></blockquote>`);
      return;
    }
    if (line.trim() === "") {
      flush();
      close();
      return;
    }
    close();
    paragraph.push(line.trim());
  });
  if (fence && fenceLanguage !== null) {
    // An unclosed fence runs to the end of the document.
    const fenced = makeFence(fenceLanguage, code, fenceLine, lines.length + skipped);
    diagrams.push(fenced);
    html.push(placeholder(fenced));
  }
  flush();
  close();
  return { html: html.join("\n"), frontmatter, headings, diagnostics, diagrams };
}

/** Typst text escapes, as `typst.rs` `escape_typst_text`. */
function escapeTypst(text: string): string {
  return text.replace(/[#$@[\]]/g, (c) => `\\${c}`);
}

/** Inline Markdown to Typst: code, strong, emphasis and links. */
function inlineTypst(text: string): string {
  const parts = text.split(/(`[^`]+`)/);
  return parts
    .map((part) => {
      if (/^`[^`]+`$/.test(part)) return part;
      return escapeTypst(part)
        .replace(/\\\[([^\]]+?)\\\]\(([^)\s]+)\)/g, '#link("$2")[$1]')
        .replace(/\*\*([^*]+)\*\*/g, "\uE000$1\uE000")
        .replace(/\*([^*]+)\*/g, "_$1_")
        .replace(/\uE000/g, "*");
    })
    .join("");
}

/**
 * Markdown to Typst markup, for the block subset `render` knows. Frontmatter
 * is dropped with an info diagnostic, like the real converter.
 */
export function toTypst(source: string): {
  typst: string;
  diagnostics: MarkdownDiagnostic[];
} {
  const { body, frontmatter } = split(source);
  const diagnostics: MarkdownDiagnostic[] = [];
  if (frontmatter !== null) {
    diagnostics.push({
      severity: "info",
      message: "YAML frontmatter was extracted and omitted from Typst body",
      line: 1,
    });
  }
  const out: string[] = [];
  let fence = false;
  for (const raw of body.split("\n")) {
    const line = raw.replace(/\r$/, "");
    if (line.startsWith("```")) {
      out.push(line);
      fence = !fence;
      continue;
    }
    if (fence) {
      out.push(line);
      continue;
    }
    const heading = /^(#{1,6})\s+(.*?)\s*#*$/.exec(line);
    if (heading !== null) {
      out.push(
        `${"=".repeat((heading[1] ?? "#").length)} ${inlineTypst(heading[2] ?? "")}`,
      );
      continue;
    }
    const item = /^(\s*)([-*+]|\d+\.)\s+(.*)$/.exec(line);
    if (item !== null) {
      const marker = /\d/.test(item[2] ?? "") ? "+" : "-";
      out.push(`${item[1] ?? ""}${marker} ${inlineTypst(item[3] ?? "")}`);
      continue;
    }
    if (/^\s*([-*_])\1\1+\s*$/.test(line)) {
      out.push("#line(length: 100%)");
      continue;
    }
    const quote = /^>\s?(.*)$/.exec(line);
    if (quote !== null) {
      out.push(`#quote(block: true)[${inlineTypst(quote[1] ?? "")}]`);
      continue;
    }
    out.push(inlineTypst(line));
  }
  const typst = `${out.join("\n").trim()}\n`;
  return { typst, diagnostics };
}

export function markdownPlugin(): MockPluginFactory {
  return (): MockPlugin => ({
    name: "markdown",
    commands: COMMANDS,
    handlers: {
      "markdown.render-html": (args): MarkdownRender => {
        const source = args["source"];
        if (typeof source !== "string") throw { message: "`source` must be a string" };
        return render(source);
      },
      "markdown.outline": (args) => {
        const source = args["source"];
        if (typeof source !== "string") throw { message: "`source` must be a string" };
        return { headings: render(source).headings };
      },
      "markdown.to-typst": (args) => {
        const source = args["source"];
        if (typeof source !== "string") throw { message: "`source` must be a string" };
        return toTypst(source);
      },
      "markdown.diagrams": (args) => {
        const source = args["source"];
        if (typeof source !== "string") throw { message: "`source` must be a string" };
        return { diagrams: render(source).diagrams };
      },
    },
  });
}
