/**
 * In-memory `search` plugin.
 *
 * Reproduces `crates/manang-search` command surface (files, text, cancel,
 * replace-preview, replace-apply, in-text) over `MockProjectHost` in browser/tests.
 */

import { fuzzyRank } from "$lib/commands/fuzzy";
import type { MockPlugin, MockPluginFactory } from "./mock";
import type { MockProjectFs, MockProjectHost } from "./mockProject";
import type {
  SearchApplyFileResult,
  SearchApplyResponse,
  SearchFileMatches,
  SearchFileResult,
  SearchFilesResponse,
  SearchLineMatch,
  SearchPreviewEdit,
  SearchPreviewFile,
  SearchPreviewResponse,
  SearchRange,
  SearchStats,
  SearchTextResponse,
} from "./api";

const COMMANDS: readonly (readonly [string, string])[] = [
  ["search.cancel", "Cancel running search"],
  ["search.files", "Go to file (fuzzy file-name search)"],
  ["search.in-text", "Find in document text"],
  ["search.replace-apply", "Apply planned replace"],
  ["search.replace-preview", "Preview replace across project"],
  ["search.text", "Search in project"],
];

function str(args: Record<string, unknown>, key: string): string {
  const value = args[key];
  if (typeof value !== "string") {
    throw { message: `bad arguments: \`${key}\` must be a string` };
  }
  return value;
}

function optBool(args: Record<string, unknown>, key: string): boolean {
  const value = args[key];
  return typeof value === "boolean" ? value : false;
}

function optStringArray(args: Record<string, unknown>, key: string): string[] {
  const value = args[key];
  if (Array.isArray(value)) {
    return value.filter((x): x is string => typeof x === "string");
  }
  return [];
}

function optUint(args: Record<string, unknown>, key: string): number | undefined {
  const value = args[key];
  if (typeof value === "number" && Number.isInteger(value) && value >= 0) {
    return value;
  }
  return undefined;
}

function contentHash(content: string): string {
  let h = 0x811c9dc5;
  for (let i = 0; i < content.length; i += 1) {
    h ^= content.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return (h >>> 0).toString(16).padStart(8, "0");
}

function matchesGlob(path: string, glob: string): boolean {
  const trimmed = glob.trim();
  if (trimmed === "") return false;
  const rePattern = trimmed
    .replace(/[.+^${}()|[\]\\]/g, "\\$&")
    .replace(/\*\*/g, ".*")
    .replace(/\*/g, "[^/]*")
    .replace(/\?/g, ".");
  const re = new RegExp(`^${rePattern}$`);
  const filename = path.split("/").pop() ?? path;
  return re.test(path) || re.test(filename);
}

function filterPath(path: string, include: readonly string[], exclude: readonly string[]): boolean {
  if (include.length > 0) {
    const included = include.some((g) => matchesGlob(path, g));
    if (!included) return false;
  }
  if (exclude.length > 0) {
    const excluded = exclude.some((g) => matchesGlob(path, g));
    if (excluded) return false;
  }
  return true;
}

function buildRegex(query: string, regex: boolean, caseSensitive: boolean, wholeWord: boolean): RegExp {
  let pattern = regex ? query : query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  if (wholeWord) {
    pattern = `\\b(?:${pattern})\\b`;
  }
  const flags = caseSensitive ? "g" : "gi";
  try {
    return new RegExp(pattern, flags);
  } catch (error) {
    throw { message: `bad regex: ${error instanceof Error ? error.message : String(error)}` };
  }
}

interface StoredPlan {
  readonly id: string;
  readonly files: Map<string, { hash: string; newContent: string; count: number }>;
}

export function searchPlugin(host: MockProjectHost): MockPluginFactory {
  let nextTicketId = 1;
  let nextPlanId = 1;
  const storedPlans = new Map<string, StoredPlan>();

  function resolveFs(rootArg: string): MockProjectFs {
    const current = host.current;
    if (current === null || current.root !== rootArg) {
      try {
        return host.open(rootArg);
      } catch {
        throw { message: `project root ${rootArg} not found` };
      }
    }
    return current;
  }

  return (emit): MockPlugin => ({
    name: "search",
    commands: COMMANDS,
    handlers: {
      "search.files": (args) => {
        const root = str(args, "root");
        const query = str(args, "query");
        const limit = optUint(args, "limit") ?? 50;
        const hidden = optBool(args, "hidden");

        const fs = resolveFs(root);
        const paths = [...fs.files.keys()].filter((p) => {
          if (!hidden && (p.startsWith(".") || p.includes("/."))) return false;
          return true;
        });

        const ranked = fuzzyRank(paths, query, (p) => p);
        const results: SearchFileResult[] = ranked.slice(0, limit).map((r) => {
          const nameStart = r.item.lastIndexOf("/") + 1;
          return {
            path: r.item,
            name_start: nameStart >= 0 ? nameStart : 0,
            score: r.score,
            indices: r.indices,
          };
        });

        const response: SearchFilesResponse = {
          results,
          total_indexed: paths.length,
          index_truncated: false,
          elapsed_ms: 1.0,
        };
        return response;
      },

      "search.text": (args) => {
        const root = str(args, "root");
        const query = str(args, "query");
        const regex = optBool(args, "regex");
        const caseSensitive = optBool(args, "case_sensitive");
        const wholeWord = optBool(args, "whole_word");
        const include = optStringArray(args, "include");
        const exclude = optStringArray(args, "exclude");
        const limit = optUint(args, "limit") ?? 500;
        const contextLines = optUint(args, "context_lines") ?? 0;
        const hidden = optBool(args, "hidden");

        const ticketId = nextTicketId++;
        const emptyStats: SearchStats = {
          bytes_scanned: 0,
          files_searched: 0,
          files_matched: 0,
          files_binary: 0,
          files_unreadable: 0,
          files_too_large: 0,
          duration_ms: 1.0,
        };

        if (query === "") {
          const emptyResponse: SearchTextResponse = {
            id: ticketId,
            files: [],
            total_matches: 0,
            total_ranges: 0,
            truncated: false,
            cancelled: false,
            stopped: null,
            stats: emptyStats,
          };
          return emptyResponse;
        }

        const matcher = buildRegex(query, regex, caseSensitive, wholeWord);
        const fs = resolveFs(root);
        const fileMatches: SearchFileMatches[] = [];
        let totalMatches = 0;
        let totalRanges = 0;
        let bytesScanned = 0;
        let filesSearched = 0;
        let filesMatched = 0;

        for (const [path, content] of fs.files.entries()) {
          if (!hidden && (path.startsWith(".") || path.includes("/."))) continue;
          if (!filterPath(path, include, exclude)) continue;
          if (typeof content !== "string") continue;

          filesSearched += 1;
          bytesScanned += content.length;
          const lines = content.split("\n");
          const matches: SearchLineMatch[] = [];
          let byteOffset = 0;

          for (let lineIdx = 0; lineIdx < lines.length; lineIdx += 1) {
            const lineText = lines[lineIdx] ?? "";
            matcher.lastIndex = 0;
            const ranges: SearchRange[] = [];
            let firstCol = 1;
            let m: RegExpExecArray | null;

            while ((m = matcher.exec(lineText)) !== null) {
              const start = m.index;
              const end = m.index + m[0].length;
              if (ranges.length === 0) {
                firstCol = start + 1;
              }
              ranges.push({ start, end });
              if (m[0].length === 0) {
                matcher.lastIndex += 1;
              }
              if (ranges.length >= 100) break;
            }

            if (ranges.length > 0) {
              const beforeLines = [];
              for (let b = Math.max(0, lineIdx - contextLines); b < lineIdx; b += 1) {
                beforeLines.push({ line: b + 1, text: lines[b] ?? "" });
              }
              const afterLines = [];
              for (let a = lineIdx + 1; a <= Math.min(lines.length - 1, lineIdx + contextLines); a += 1) {
                afterLines.push({ line: a + 1, text: lines[a] ?? "" });
              }

              matches.push({
                line: lineIdx + 1,
                column: firstCol,
                byte_offset: byteOffset,
                text: lineText,
                text_offset: 0,
                ranges,
                before: beforeLines,
                after: afterLines,
              });

              totalMatches += 1;
              totalRanges += ranges.length;
              if (totalMatches >= limit) break;
            }

            byteOffset += lineText.length + 1;
          }

          if (matches.length > 0) {
            filesMatched += 1;
            fileMatches.push({
              path,
              matches,
              truncated: false,
            });
          }

          if (totalMatches >= limit) break;
        }

        const response: SearchTextResponse = {
          id: ticketId,
          files: fileMatches,
          total_matches: totalMatches,
          total_ranges: totalRanges,
          truncated: totalMatches >= limit,
          cancelled: false,
          stopped: null,
          stats: {
            bytes_scanned: bytesScanned,
            files_searched: filesSearched,
            files_matched: filesMatched,
            files_binary: 0,
            files_unreadable: 0,
            files_too_large: 0,
            duration_ms: 2.0,
          },
        };
        return response;
      },

      "search.cancel": () => {
        return { cancelled: 1 };
      },

      "search.replace-preview": (args) => {
        const root = str(args, "root");
        const query = str(args, "query");
        const replacement = str(args, "replacement");
        const regex = optBool(args, "regex");
        const caseSensitive = optBool(args, "case_sensitive");
        const wholeWord = optBool(args, "whole_word");
        const include = optStringArray(args, "include");
        const exclude = optStringArray(args, "exclude");
        const limit = optUint(args, "limit") ?? 500;
        const hidden = optBool(args, "hidden");

        const ticketId = nextTicketId++;
        const planId = `plan-${nextPlanId++}`;
        const matcher = buildRegex(query, regex, caseSensitive, wholeWord);
        const fs = resolveFs(root);

        const previewFiles: SearchPreviewFile[] = [];
        const planMap = new Map<string, { hash: string; newContent: string; count: number }>();
        let totalEdits = 0;
        let filesSearched = 0;
        let filesMatched = 0;
        let bytesScanned = 0;

        for (const [path, content] of fs.files.entries()) {
          if (!hidden && (path.startsWith(".") || path.includes("/."))) continue;
          if (!filterPath(path, include, exclude)) continue;
          if (typeof content !== "string") continue;

          filesSearched += 1;
          bytesScanned += content.length;
          const hash = contentHash(content);
          const lines = content.split("\n");
          const edits: SearchPreviewEdit[] = [];
          const newLines: string[] = [];
          let fileEdits = 0;
          let fileOffset = 0;

          for (let lineIdx = 0; lineIdx < lines.length; lineIdx += 1) {
            const lineText = lines[lineIdx] ?? "";
            matcher.lastIndex = 0;
            let m: RegExpExecArray | null;
            let newLineText = "";
            let lastIdx = 0;

            while ((m = matcher.exec(lineText)) !== null) {
              const matchedStr = m[0];
              const start = m.index;
              const end = m.index + matchedStr.length;
              newLineText += lineText.slice(lastIdx, start) + replacement;
              lastIdx = end;

              edits.push({
                line: lineIdx + 1,
                column: start + 1,
                start,
                end,
                offset_start: fileOffset + start,
                offset_end: fileOffset + end,
                matched: matchedStr,
                replacement,
                line_text: lineText,
              });

              fileEdits += 1;
              totalEdits += 1;
              if (matchedStr.length === 0) {
                matcher.lastIndex += 1;
              }
              if (totalEdits >= limit) break;
            }

            newLineText += lineText.slice(lastIdx);
            newLines.push(newLineText);
            fileOffset += lineText.length + 1;
            if (totalEdits >= limit) break;
          }

          if (edits.length > 0) {
            filesMatched += 1;
            previewFiles.push({
              path,
              hash,
              edits,
            });
            planMap.set(path, {
              hash,
              newContent: newLines.join("\n"),
              count: fileEdits,
            });
          }

          if (totalEdits >= limit) break;
        }

        storedPlans.set(planId, { id: planId, files: planMap });

        const response: SearchPreviewResponse = {
          id: ticketId,
          plan_id: planId,
          files: previewFiles,
          total_edits: totalEdits,
          truncated: totalEdits >= limit,
          cancelled: false,
          stopped: null,
          stats: {
            bytes_scanned: bytesScanned,
            files_searched: filesSearched,
            files_matched: filesMatched,
            files_binary: 0,
            files_unreadable: 0,
            files_too_large: 0,
            duration_ms: 2.0,
          },
        };
        return response;
      },

      "search.replace-apply": (args) => {
        const planId = str(args, "plan_id");
        const fileFilter = Array.isArray(args["files"]) ? (args["files"] as string[]) : undefined;
        const plan = storedPlans.get(planId);
        if (plan === undefined) {
          throw { message: `unknown or expired plan \`${planId}\`` };
        }
        storedPlans.delete(planId);

        const currentFs = host.current;
        if (currentFs === null) {
          throw { message: "no project is open" };
        }

        const results: SearchApplyFileResult[] = [];
        let applied = 0;
        let failed = 0;

        for (const [path, entry] of plan.files.entries()) {
          if (fileFilter && !fileFilter.includes(path)) continue;
          const currentContent = currentFs.files.get(path);
          if (typeof currentContent !== "string") {
            failed += 1;
            results.push({
              path,
              ok: false,
              replacements: 0,
              error: "file not found or binary",
            });
            continue;
          }
          if (contentHash(currentContent) !== entry.hash) {
            failed += 1;
            results.push({
              path,
              ok: false,
              replacements: 0,
              error: "the file changed since the preview; search again",
            });
            continue;
          }

          currentFs.files.set(path, entry.newContent);
          applied += 1;
          results.push({
            path,
            ok: true,
            replacements: entry.count,
          });
        }

        if (applied > 0) {
          emit("project.changed", {
            root: currentFs.root,
            changes: results
              .filter((r) => r.ok)
              .map((r) => ({ path: r.path, kind: "modified" })),
          });
        }

        const response: SearchApplyResponse = {
          applied,
          failed,
          files: results,
        };
        return response;
      },

      "search.in-text": (args) => {
        const text = str(args, "text");
        const query = str(args, "query");
        const regex = optBool(args, "regex");
        const caseSensitive = optBool(args, "case_sensitive");
        const wholeWord = optBool(args, "whole_word");
        const limit = optUint(args, "limit") ?? 500;

        if (query === "") {
          return {
            id: 0,
            files: [],
            total_matches: 0,
            total_ranges: 0,
            truncated: false,
            cancelled: false,
            stopped: null,
            stats: {
              bytes_scanned: 0,
              files_searched: 1,
              files_matched: 0,
              files_binary: 0,
              files_unreadable: 0,
              files_too_large: 0,
              duration_ms: 1.0,
            },
          };
        }

        const matcher = buildRegex(query, regex, caseSensitive, wholeWord);
        const lines = text.split("\n");
        const matches: SearchLineMatch[] = [];
        let totalRanges = 0;
        let byteOffset = 0;

        for (let lineIdx = 0; lineIdx < lines.length; lineIdx += 1) {
          const lineText = lines[lineIdx] ?? "";
          matcher.lastIndex = 0;
          const ranges: SearchRange[] = [];
          let firstCol = 1;
          let m: RegExpExecArray | null;

          while ((m = matcher.exec(lineText)) !== null) {
            const start = m.index;
            const end = m.index + m[0].length;
            if (ranges.length === 0) firstCol = start + 1;
            ranges.push({ start, end });
            if (m[0].length === 0) matcher.lastIndex += 1;
            if (ranges.length >= 100) break;
          }

          if (ranges.length > 0) {
            matches.push({
              line: lineIdx + 1,
              column: firstCol,
              byte_offset: byteOffset,
              text: lineText,
              text_offset: 0,
              ranges,
              before: [],
              after: [],
            });
            totalRanges += ranges.length;
            if (matches.length >= limit) break;
          }
          byteOffset += lineText.length + 1;
        }

        return {
          id: 0,
          files: matches.length > 0 ? [{ path: "", matches, truncated: false }] : [],
          total_matches: matches.length,
          total_ranges: totalRanges,
          truncated: matches.length >= limit,
          cancelled: false,
          stopped: null,
          stats: {
            bytes_scanned: text.length,
            files_searched: 1,
            files_matched: matches.length > 0 ? 1 : 0,
            files_binary: 0,
            files_unreadable: 0,
            files_too_large: 0,
            duration_ms: 1.0,
          },
        };
      },
    },
  });
}
