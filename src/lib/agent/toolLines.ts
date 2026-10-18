/**
 * Tool calls in words. The chat never shows tool names or JSON: consecutive calls fold into one
 * quiet row ("Read 3 files, searched "Fourier""), a running call is one line that is replaced
 * by the next, and a failure stays visible.
 */

import type { ToolRow } from "./reducer";

function arg(row: ToolRow, key: string): string {
  const v = row.args[key];
  return typeof v === "string" ? v : "";
}

function quoted(text: string): string {
  const short = text.length > 28 ? `${text.slice(0, 27)}…` : text;
  return `“${short}”`;
}

/** One call as a sentence: past tense when done, present while it runs. */
export function rowLine(row: ToolRow): string {
  const running = row.status === "running";
  const path = arg(row, "path");
  switch (row.name) {
    case "list_files":
      return running ? "Listing files" : "Listed files";
    case "read_file":
      return `${running ? "Reading" : "Read"} ${path || "a file"}`;
    case "search":
      return `${running ? "Searching for" : "Searched for"} ${quoted(arg(row, "query"))}`;
    case "outline":
      return path
        ? `${running ? "Reading" : "Read"} the outline of ${path}`
        : `${running ? "Reading" : "Read"} the outlines`;
    case "diagnostics":
      return running ? "Checking the problems" : "Checked the problems";
    case "bib_search":
      return `${running ? "Searching" : "Searched"} the references for ${quoted(arg(row, "query"))}`;
    case "verify_citation":
      return running ? "Verifying a citation" : "Verified a citation";
    case "edit_text":
      return "An edit was not accepted";
    default:
      return `${running ? "Running" : "Ran"} ${row.name.replace(/_/g, " ")}`;
  }
}

/** A failed call's reason, for the expanded list. */
export function rowDetail(row: ToolRow): string {
  return row.status === "error" ? row.summary : "";
}

/** The folded row for a group of consecutive calls. */
export function groupLine(rows: readonly ToolRow[]): string {
  const running = [...rows].reverse().find((r) => r.status === "running");
  if (running !== undefined) return `${rowLine(running)}…`;
  const failed = rows.filter((r) => r.status === "error").length;
  const reads = new Set(
    rows.filter((r) => r.name === "read_file").map((r) => arg(r, "path")),
  );
  const searches = rows.filter((r) => r.name === "search");
  const parts: string[] = [];
  if (reads.size === 1) parts.push(`read ${[...reads][0] ?? "a file"}`);
  else if (reads.size > 1) parts.push(`read ${reads.size} files`);
  if (searches.length === 1)
    parts.push(`searched for ${quoted(arg(searches[0]!, "query"))}`);
  else if (searches.length > 1) parts.push(`searched ${searches.length} times`);
  const seen = new Set<string>();
  for (const r of rows) {
    if (r.name === "read_file" || r.name === "search" || r.name === "edit_text")
      continue;
    if (seen.has(r.name)) continue;
    seen.add(r.name);
    parts.push(rowLine({ ...r, status: "done" }).toLowerCase());
  }
  if (failed > 0) parts.push(`${failed} failed`);
  const text = parts.join(", ");
  return text === "" ? "Worked" : text.charAt(0).toUpperCase() + text.slice(1);
}
