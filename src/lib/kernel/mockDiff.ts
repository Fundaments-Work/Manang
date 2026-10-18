/**
 * The browser stand-in for `crates/manang-project/src/diff.rs`: a line diff of
 * two texts as `git.diff`-shaped hunks, plus the fingerprint the mock project
 * uses where Rust uses BLAKE3. The browser mock only needs the same contract,
 * not the same algorithm.
 */

import type { TextDiff } from "./api";

/** Inputs with more lines than this (per side) are not diffed. */
const MAX_LINES = 1500;
const CONTEXT = 3;

/** A 53-bit string hash (cyrb53), spelled like the Rust fingerprint. */
export function mockHash(text: string): string {
  let h1 = 0xdeadbeef;
  let h2 = 0x41c6ce57;
  for (let i = 0; i < text.length; i += 1) {
    const ch = text.charCodeAt(i);
    h1 = Math.imul(h1 ^ ch, 2654435761);
    h2 = Math.imul(h2 ^ ch, 1597334677);
  }
  h1 =
    Math.imul(h1 ^ (h1 >>> 16), 2246822507) ^ Math.imul(h2 ^ (h2 >>> 13), 3266489909);
  h2 =
    Math.imul(h2 ^ (h2 >>> 16), 2246822507) ^ Math.imul(h1 ^ (h1 >>> 13), 3266489909);
  const n = 4294967296 * (2097151 & h2) + (h1 >>> 0);
  return `b3:${n.toString(16).padStart(14, "0")}${text.length.toString(16).padStart(4, "0")}`;
}

function lines(text: string): string[] {
  if (text === "") return [];
  const parts = text.split("\n");
  if (parts[parts.length - 1] === "") parts.pop();
  return parts;
}

type Op = { kind: "context" | "add" | "remove"; text: string; a: number; b: number };

/** The edit script from `a` to `b` by longest common subsequence. */
function script(a: readonly string[], b: readonly string[]): Op[] {
  const n = a.length;
  const m = b.length;
  const table: number[][] = Array.from({ length: n + 1 }, () =>
    new Array<number>(m + 1).fill(0),
  );
  for (let i = n - 1; i >= 0; i -= 1) {
    for (let j = m - 1; j >= 0; j -= 1) {
      const row = table[i];
      const next = table[i + 1];
      if (row === undefined || next === undefined) continue;
      row[j] =
        a[i] === b[j]
          ? (next[j + 1] ?? 0) + 1
          : Math.max(next[j] ?? 0, row[j + 1] ?? 0);
    }
  }
  const ops: Op[] = [];
  let i = 0;
  let j = 0;
  while (i < n || j < m) {
    if (i < n && j < m && a[i] === b[j]) {
      ops.push({ kind: "context", text: a[i] ?? "", a: i + 1, b: j + 1 });
      i += 1;
      j += 1;
    } else if (
      i < n &&
      (j === m || (table[i + 1]?.[j] ?? 0) >= (table[i]?.[j + 1] ?? 0))
    ) {
      ops.push({ kind: "remove", text: a[i] ?? "", a: i + 1, b: 0 });
      i += 1;
    } else {
      ops.push({ kind: "add", text: b[j] ?? "", a: 0, b: j + 1 });
      j += 1;
    }
  }
  return ops;
}

/** Diff the file on disk (`old`, null when missing) against `next`. */
export function mockDiffText(path: string, old: string | null, next: string): TextDiff {
  const base = {
    path,
    status: old === null ? ("added" as const) : ("modified" as const),
    binary: false,
  };
  const a = lines(old ?? "");
  const b = lines(next);
  if (a.length > MAX_LINES || b.length > MAX_LINES) {
    return { ...base, too_large: true, additions: 0, deletions: 0, hunks: [] };
  }
  const ops = script(a, b);
  const changed = ops.flatMap((op, index) => (op.kind === "context" ? [] : [index]));
  const hunks: TextDiff["hunks"][number][] = [];
  let additions = 0;
  let deletions = 0;
  let from = 0;
  while (from < changed.length) {
    let to = from;
    while (
      to + 1 < changed.length &&
      (changed[to + 1] ?? 0) - (changed[to] ?? 0) <= CONTEXT * 2 + 1
    )
      to += 1;
    const start = Math.max(0, (changed[from] ?? 0) - CONTEXT);
    const end = Math.min(ops.length, (changed[to] ?? 0) + CONTEXT + 1);
    const slice = ops.slice(start, end);
    const oldFirst = slice.find((o) => o.a > 0)?.a ?? 0;
    const newFirst = slice.find((o) => o.b > 0)?.b ?? 0;
    const oldCount = slice.filter((o) => o.kind !== "add").length;
    const newCount = slice.filter((o) => o.kind !== "remove").length;
    for (const op of slice) {
      if (op.kind === "add") additions += 1;
      if (op.kind === "remove") deletions += 1;
    }
    hunks.push({
      old_start: oldFirst,
      old_lines: oldCount,
      new_start: newFirst,
      new_lines: newCount,
      header: `@@ -${oldFirst},${oldCount} +${newFirst},${newCount} @@`,
      lines: slice.map((op) => ({
        kind: op.kind,
        text: op.text,
        old_line: op.a > 0 ? op.a : null,
        new_line: op.b > 0 ? op.b : null,
        old_offset: null,
        new_offset: null,
        no_newline: false,
      })),
    });
    from = to + 1;
  }
  return { ...base, too_large: false, additions, deletions, hunks };
}
