/**
 * Outline extraction for Typst, LaTeX and Markdown documents.
 *
 * Fast, pure TypeScript parsers for Typst (= headings and <labels>) and
 * LaTeX (\section..\subsubsection and \label{...}), with Markdown support
 * delegating to `markdown.outline` (with a pure TS fallback).
 */

import type { Kernel } from "$lib/kernel";
import { markdownApi } from "$lib/kernel/api";

export interface OutlineItem {
  readonly title: string;
  readonly level: number;
  readonly line: number;
  readonly column: number;
  readonly id?: string | undefined;
  readonly label?: string | undefined;
  readonly offset?: number | undefined;
}

/**
 * Extracts headings and labels from Typst source text.
 * Typst headings start with one or more `=` characters followed by space and the title.
 * Labels `<label-name>` can appear on the same line or immediately after.
 */
export function parseTypstOutline(source: string): OutlineItem[] {
  const items: OutlineItem[] = [];
  const lines = source.split("\n");
  let inCodeBlock = false;
  let inBlockComment = false;

  for (let i = 0; i < lines.length; i += 1) {
    const rawLine = lines[i] ?? "";
    const trimmed = rawLine.trim();

    // Track block comments
    if (inBlockComment) {
      if (trimmed.includes("*/")) {
        inBlockComment = false;
      }
      continue;
    }
    if (trimmed.startsWith("/*")) {
      if (!trimmed.includes("*/")) {
        inBlockComment = true;
      }
      continue;
    }

    // Track code blocks
    if (trimmed.startsWith("```")) {
      inCodeBlock = !inCodeBlock;
      continue;
    }
    if (inCodeBlock) continue;

    // Ignore single line comments
    if (trimmed.startsWith("//")) continue;

    // Check for heading: starts with =
    const headingMatch = /^(=+)\s+(.+)$/.exec(trimmed);
    if (!headingMatch) continue;

    const eqCount = headingMatch[1]?.length ?? 1;
    const content = headingMatch[2]?.trim() ?? "";

    // Extract inline label: <label_name>
    let title = content;
    let label: string | undefined;

    const labelMatch = /<([A-Za-z0-9_:-]+)>\s*$/.exec(content);
    if (labelMatch) {
      label = labelMatch[1];
      title = content.slice(0, labelMatch.index).trim();
    } else if (i + 1 < lines.length) {
      // Check next line for standalone <label>
      const nextTrimmed = (lines[i + 1] ?? "").trim();
      const nextLabelMatch = /^<([A-Za-z0-9_:-]+)>$/.exec(nextTrimmed);
      if (nextLabelMatch) {
        label = nextLabelMatch[1];
      }
    }

    const column = rawLine.indexOf("=") + 1;

    items.push({
      title,
      level: Math.min(6, eqCount),
      line: i + 1,
      column: Math.max(1, column),
      id: label,
      label,
    });
  }

  return items;
}

const LATEX_SECTION_LEVELS: Record<string, number> = {
  part: 1,
  chapter: 1,
  section: 2,
  subsection: 3,
  subsubsection: 4,
  paragraph: 5,
  subparagraph: 6,
};

function cleanLatexTitle(raw: string): string {
  return raw
    .replace(/\\[A-Za-z]+(\[[^\]]*\])?\{([^}]*)\}/g, "$2")
    .replace(/\\[A-Za-z]+/g, "")
    .replace(/[{}]/g, "")
    .replace(/\s+/g, " ")
    .trim();
}

/**
 * Extracts headings from LaTeX source text (\section..\subsubsection and \label{...}).
 */
export function parseLatexOutline(source: string): OutlineItem[] {
  const items: OutlineItem[] = [];
  const lines = source.split("\n");
  let inVerbatim = false;

  const sectionRegex =
    /\\(part|chapter|section|subsection|subsubsection|paragraph|subparagraph)\*?(?:\[[^\]]*\])?\{/;

  for (let i = 0; i < lines.length; i += 1) {
    const rawLine = lines[i] ?? "";
    const trimmed = rawLine.trim();

    // Verbatim environments
    if (/\\begin\{(verbatim|lstlisting|minted)\}/.test(trimmed)) {
      inVerbatim = true;
      continue;
    }
    if (/\\end\{(verbatim|lstlisting|minted)\}/.test(trimmed)) {
      inVerbatim = false;
      continue;
    }
    if (inVerbatim) continue;

    // Ignore pure comments
    if (trimmed.startsWith("%")) continue;

    // Remove comment part from line
    const commentIdx = rawLine.indexOf("%");
    const lineWithoutComment = commentIdx !== -1 ? rawLine.slice(0, commentIdx) : rawLine;

    const match = sectionRegex.exec(lineWithoutComment);
    if (!match) continue;

    const cmd = match[1] ?? "section";
    const level = LATEX_SECTION_LEVELS[cmd] ?? 2;
    const startIdx = match.index + match[0].length;

    // Parse balanced braces for heading title
    let depth = 1;
    let titleEnd = startIdx;
    let titleStr = "";

    // Search across line and subsequent lines if needed
    let lineIdx = i;
    let curLine = lineWithoutComment;
    let curOffset = startIdx;

    while (lineIdx < lines.length && depth > 0) {
      for (let c = curOffset; c < curLine.length; c += 1) {
        if (curLine[c] === "{") depth += 1;
        else if (curLine[c] === "}") {
          depth -= 1;
          if (depth === 0) {
            titleEnd = c;
            titleStr += curLine.slice(curOffset, c);
            break;
          }
        }
      }
      if (depth > 0) {
        titleStr += curLine.slice(curOffset) + " ";
        lineIdx += 1;
        if (lineIdx < lines.length) {
          curLine = lines[lineIdx] ?? "";
          curOffset = 0;
        }
      }
    }

    const title = cleanLatexTitle(titleStr);

    // Look for \label{...} on remainder of line or next line
    let label: string | undefined;
    const afterTitle = curLine.slice(titleEnd + 1);
    const labelMatch = /\\label\{([^}]+)\}/.exec(afterTitle);
    if (labelMatch) {
      label = labelMatch[1]?.trim();
    } else if (lineIdx + 1 < lines.length) {
      const nextLine = (lines[lineIdx + 1] ?? "").trim();
      const nextLabelMatch = /^\\label\{([^}]+)\}/.exec(nextLine);
      if (nextLabelMatch) {
        label = nextLabelMatch[1]?.trim();
      }
    }

    const column = match.index + 1;

    items.push({
      title,
      level,
      line: i + 1,
      column,
      id: label,
      label,
    });
  }

  return items;
}

/**
 * Pure TypeScript fallback parser for Markdown headings.
 */
export function parseMarkdownOutlineTS(source: string): OutlineItem[] {
  const items: OutlineItem[] = [];
  const lines = source.split("\n");
  let inCodeBlock = false;

  for (let i = 0; i < lines.length; i += 1) {
    const rawLine = lines[i] ?? "";
    const trimmed = rawLine.trim();

    if (trimmed.startsWith("```")) {
      inCodeBlock = !inCodeBlock;
      continue;
    }
    if (inCodeBlock) continue;

    // Check for ATX heading (# ...)
    const match = /^(#{1,6})\s+(.+)$/.exec(trimmed);
    if (!match) continue;

    const level = match[1]?.length ?? 1;
    let content = match[2]?.trim() ?? "";

    // Check for attribute ID e.g. {#intro}
    let id: string | undefined;
    const idMatch = /\{#([^}]+)\}\s*$/.exec(content);
    if (idMatch) {
      id = idMatch[1]?.trim();
      content = content.slice(0, idMatch.index).trim();
    }

    const column = rawLine.indexOf("#") + 1;

    items.push({
      title: content,
      level,
      line: i + 1,
      column: Math.max(1, column),
      id,
      label: id,
    });
  }

  return items;
}

/**
 * Parses markdown outline, using `markdown.outline` kernel command if available,
 * falling back to the pure TS parser.
 */
export async function parseMarkdownOutline(
  source: string,
  kernel?: Kernel,
): Promise<OutlineItem[]> {
  if (kernel) {
    try {
      const res = await markdownApi(kernel).outline(source);
      if (res && Array.isArray(res.headings)) {
        return res.headings.map((h) => ({
          title: h.title,
          level: h.level,
          line: h.line,
          column: 1,
          id: h.id,
          label: h.id,
          offset: h.offset,
        }));
      }
    } catch {
      // Fallback
    }
  }
  return parseMarkdownOutlineTS(source);
}

/**
 * Computes the outline for a document given its text and engine/syntax.
 */
export async function getDocumentOutline(
  source: string,
  engine: "typst" | "latex" | "markdown" | "plain",
  kernel?: Kernel,
): Promise<OutlineItem[]> {
  switch (engine) {
    case "typst":
      return parseTypstOutline(source);
    case "latex":
      return parseLatexOutline(source);
    case "markdown":
      return parseMarkdownOutline(source, kernel);
    case "plain":
    default:
      return [];
  }
}
