/**
 * Margin Whispers — DESIGN.md §3.2.
 *
 * Diagnostics never raise modals or dump a terminal. Each one becomes a 4px
 * square in the line-number gutter, and hovering reveals a single quiet line
 * with a one-click fix where one exists.
 *
 * Pure functions over the manuscript text: the Rust core will eventually own
 * the authoritative parse, but the shell needs immediate feedback on every
 * keystroke and cannot afford a round trip per character.
 */

export type Severity = "error" | "warning";

export interface QuickFix {
  readonly title: string;
  /** Returns the corrected text. Must be a pure function of its input. */
  readonly apply: (text: string) => string;
}

export interface Diagnostic {
  readonly from: number;
  readonly to: number;
  readonly message: string;
  readonly severity: Severity;
  readonly fix?: QuickFix;
}

export interface DiagnosticOptions {
  /** Figure paths known to exist, e.g. from the Library shelf. */
  readonly knownFigures?: readonly string[];
}

/** Offsets of every `#label("…")` definition in the document. */
export function definedLabels(text: string): Set<string> {
  const labels = new Set<string>();
  const pattern = /#label\(\s*"([^"]+)"\s*\)/g;
  for (const match of text.matchAll(pattern)) {
    const label = match[1];
    if (label !== undefined) labels.add(label);
  }
  return labels;
}

/** Strips fenced code so diagnostics do not fire inside examples. */
function withoutFencedCode(text: string): string {
  return text.replace(/```[\s\S]*?```/g, (block) => block.replace(/[^\n]/g, " "));
}

export function lineOf(text: string, offset: number): number {
  let line = 1;
  const limit = Math.min(offset, text.length);
  for (let index = 0; index < limit; index += 1) {
    if (text[index] === "\n") line += 1;
  }
  return line;
}

/**
 * Full diagnostic sweep for the manuscript.
 *
 * Checks are ordered from most to least specific so the gutter shows the most
 * actionable whisper first when several land on one line.
 */
export function analyse(text: string, options: DiagnosticOptions = {}): Diagnostic[] {
  const scannable = withoutFencedCode(text);
  const found: Diagnostic[] = [];

  found.push(...unbalancedMath(text, scannable));
  found.push(...unknownReferences(text, scannable));
  found.push(...unclosedGroups(text, scannable));
  found.push(...missingFigures(text, options.knownFigures ?? []));

  return found.sort((a, b) => a.from - b.from || a.to - b.to);
}

/**
 * Odd numbers of `$` delimiters.
 *
 * Typst math is `$ … $`, so an odd count on the document means a delimiter was
 * dropped. The fix appends the missing one at the end of the line, which is
 * where a reader's eye expects a half-finished formula to be closed.
 */
function unbalancedMath(text: string, scannable: string): Diagnostic[] {
  const result: Diagnostic[] = [];
  let lineStart = 0;

  for (const line of scannable.split("\n")) {
    const offset = lineStart;
    const count = (line.match(/(?<!\$)\$(?!\$)/g) ?? []).length;

    if (count % 2 === 1) {
      const at = offset + (line.lastIndexOf("$") + 1);
      result.push({
        from: at,
        to: Math.min(at + 1, text.length),
        message: "Unclosed math delimiter",
        severity: "error",
        fix: {
          title: "Close the delimiter",
          apply: (source) => `${source}$`,
        },
      });
    }

    lineStart += line.length + 1;
  }

  return result;
}

/** `@key` references with no matching `#label("key")` anywhere. */
function unknownReferences(text: string, scannable: string): Diagnostic[] {
  const labels = definedLabels(text);
  const result: Diagnostic[] = [];
  const pattern = /@([a-zA-Z][\w-]*)/g;

  for (const match of scannable.matchAll(pattern)) {
    const key = match[1];
    if (key === undefined || labels.has(key)) continue;

    const at = match.index;
    result.push({
      from: at,
      to: at + match[0].length,
      message: `Missing label for @${key}`,
      severity: "warning",
      fix: {
        title: `Define #label("${key}")`,
        apply: (source) => {
          const lineEnd = source.indexOf("\n", at);
          const insertAt = lineEnd === -1 ? source.length : lineEnd;
          return `${source.slice(0, insertAt)}\n#label("${key}")${source.slice(insertAt)}`;
        },
      },
    });
  }

  return result;
}

const GROUPS: readonly { open: string; close: string; name: string }[] = [
  { open: "#{", close: "}", name: "code block" },
  { open: "#(", close: ")", name: "content block" },
  { open: "[", close: "]", name: "content block" },
];

/** Unbalanced brackets, reported at the opening delimiter. */
function unclosedGroups(text: string, scannable: string): Diagnostic[] {
  const result: Diagnostic[] = [];

  for (const group of GROUPS) {
    const stack: number[] = [];
    for (let index = 0; index < scannable.length; index += 1) {
      if (scannable.startsWith(group.open, index)) {
        stack.push(index);
        index += group.open.length - 1;
      } else if (scannable[index] === group.close) {
        stack.pop();
      }
    }

    for (const at of stack) {
      result.push({
        from: at,
        to: Math.min(at + group.open.length, text.length),
        message: `Unclosed ${group.name} \`${group.open}\` — missing \`${group.close}\``,
        severity: "error",
      });
    }
  }

  return result;
}

/** `#image("figures/x.svg")` pointing at a file the Library does not have. */
function missingFigures(text: string, known: readonly string[]): Diagnostic[] {
  if (known.length === 0) return [];

  const available = new Set(known.map((path) => path.replace(/^\.\//, "")));
  const result: Diagnostic[] = [];
  // The call is written `#image(…)` at the top level but bare `image(…)` inside
  // a `#figure(…)` wrapper, which is exactly how DESIGN.md §4.1 emits it. Only
  // the first argument is the path; trailing arguments such as `width: 80%`
  // are common, so the pattern must not require `)` after the string.
  const pattern = /(?:#)?image\(\s*"([^"]+)"/g;

  for (const match of text.matchAll(pattern)) {
    const path = match[1];
    if (path === undefined || available.has(path)) continue;

    const at = match.index;
    result.push({
      from: at,
      to: at + match[0].length,
      message: `Figure not found: ${path}`,
      severity: "warning",
    });
  }

  return result;
}

/** Applies a fix and returns the new text, or null when the fix no longer applies. */
export function applyFix(text: string, diagnostic: Diagnostic): string | null {
  if (diagnostic.fix === undefined) return null;
  if (diagnostic.to > text.length) return null;
  return diagnostic.fix.apply(text);
}
