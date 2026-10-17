/**
 * In-memory `markdown` plugin.
 *
 * A small CommonMark subset (headings, paragraphs, lists, code, emphasis, links,
 * block quotes, rules) that escapes everything else, so the Markdown preview can
 * be exercised without Rust. Result shapes mirror `crates/manang-markdown`
 * (`plugin.rs` `markdown.render-html` and `markdown.outline`, `outline.rs`).
 * The real renderer sanitises raw HTML; this one never emits any.
 */

import type { MockPlugin, MockPluginFactory } from "./mock";
import type { MarkdownDiagnostic, MarkdownHeading, MarkdownRender } from "./types";

const COMMANDS: readonly (readonly [string, string])[] = [
  ["markdown.render-html", "Render Markdown to sanitized HTML"],
  ["markdown.outline", "Extract Markdown document outline"],
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

export function render(source: string): MarkdownRender {
  const { body, skipped, frontmatter } = split(source);
  const headings: MarkdownHeading[] = [];
  const diagnostics: MarkdownDiagnostic[] = [];
  const html: string[] = [];
  let paragraph: string[] = [];
  let list: "ul" | "ol" | null = null;
  let fence = false;
  let code: string[] = [];

  const flush = (): void => {
    if (paragraph.length > 0) html.push(`<p>${inline(paragraph.join(" "))}</p>`);
    paragraph = [];
  };
  const close = (): void => {
    if (list !== null) html.push(`</${list}>`);
    list = null;
  };

  body.split("\n").forEach((raw, index) => {
    const line = raw.replace(/\r$/, "");
    const lineNo = index + 1 + skipped;
    if (line.startsWith("```")) {
      if (fence) {
        html.push(`<pre><code>${escape(code.join("\n"))}\n</code></pre>`);
        code = [];
      } else {
        flush();
        close();
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
  flush();
  close();
  return { html: html.join("\n"), frontmatter, headings, diagnostics };
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
    },
  });
}
