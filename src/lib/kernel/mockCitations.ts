/**
 * In-memory `citations` plugin.
 *
 * Reproduces `crates/manang-citations` command surface:
 *   - citations.load
 *   - citations.search
 *   - citations.get
 *   - citations.format
 *   - citations.extract-keys
 *   - citations.verify
 * over `MockProjectHost` in browser/tests.
 */

import type { MockPlugin, MockPluginFactory } from "./mock";
import type { MockProjectHost } from "./mockProject";
import type {
  CitationAuthor,
  CitationDifference,
  CitationEntry,
  CitationFormatResponse,
  CitationLoadResponse,
  CitationParseDiagnostic,
  CitationSearchResponse,
  CitationVerificationResult,
} from "./api";

const COMMANDS: readonly (readonly [string, string])[] = [
  ["citations.configure", "Apply citation settings"],
  ["citations.extract-keys", "Extract citation keys from source text"],
  ["citations.format", "Format citation entry"],
  ["citations.get", "Get entry by key"],
  ["citations.load", "Load bibliography files into index"],
  ["citations.search", "Search citations index"],
  ["citations.verify", "Verify citation against authoritative registries"],
];

function str(args: Record<string, unknown>, key: string): string {
  const value = args[key];
  if (typeof value !== "string") {
    throw { message: `bad arguments: \`${key}\` must be a string` };
  }
  return value;
}

function optString(args: Record<string, unknown>, key: string): string | undefined {
  const value = args[key];
  return typeof value === "string" ? value : undefined;
}

function cleanText(value: string): string {
  return value
    .replace(/\\(['`"^~])\s*\{?([A-Za-z])\}?/g, "$2")
    .replace(/\\&/g, "&")
    .replace(/[{}]/g, "")
    .replace(/--+/g, "–")
    .replace(/\s+/g, " ")
    .trim();
}

function parseBibAuthors(raw: string): CitationAuthor[] {
  return raw
    .split(/\s+and\s+/i)
    .map((part) => cleanText(part))
    .filter((part) => part.length > 0)
    .map((part) => {
      const comma = part.indexOf(",");
      if (comma !== -1) {
        const family = part.slice(0, comma).trim();
        const given = part.slice(comma + 1).trim();
        return {
          name: `${given} ${family}`.trim(),
          family: family.length > 0 ? family : undefined,
          given: given.length > 0 ? given : undefined,
        };
      }
      const words = part.split(/\s+/);
      const family = words.length > 1 ? words[words.length - 1] : part;
      const given = words.length > 1 ? words.slice(0, -1).join(" ") : undefined;
      return {
        name: part,
        family: family?.length ? family : undefined,
        given: given?.length ? given : undefined,
      };
    });
}

function parseBibtexEntries(source: string): CitationEntry[] {
  const entries: CitationEntry[] = [];
  const head = /@([A-Za-z]+)\s*\{\s*([^,\s{}]+)\s*,/g;
  for (let match = head.exec(source); match !== null; match = head.exec(source)) {
    const entryType = (match[1] ?? "").toLowerCase();
    if (entryType === "comment" || entryType === "string" || entryType === "preamble") {
      continue;
    }
    const key = match[2] ?? "";
    const rawFields: Record<string, string> = {};
    let idx = head.lastIndex;

    while (idx < source.length) {
      const fieldMatch = /^\s*([A-Za-z][\w-]*)\s*=\s*/.exec(source.slice(idx));
      if (!fieldMatch) break;
      idx += fieldMatch[0].length;
      const fieldName = (fieldMatch[1] ?? "").toLowerCase();

      // Read value
      let value = "";
      if (source[idx] === "{") {
        let depth = 0;
        const start = idx + 1;
        for (let i = idx; i < source.length; i += 1) {
          if (source[i] === "{") depth += 1;
          else if (source[i] === "}") {
            depth -= 1;
            if (depth === 0) {
              value = source.slice(start, i);
              idx = i + 1;
              break;
            }
          }
        }
      } else if (source[idx] === '"') {
        let i = idx + 1;
        while (i < source.length && !(source[i] === '"' && source[i - 1] !== "\\")) {
          i += 1;
        }
        value = source.slice(idx + 1, i);
        idx = i + 1;
      } else {
        const bare = /^[^,}\s]*/.exec(source.slice(idx));
        value = bare?.[0] ?? "";
        idx += value.length;
      }

      rawFields[fieldName] = cleanText(value);
      const comma = /^\s*,/.exec(source.slice(idx));
      if (comma) {
        idx += comma[0].length;
      } else {
        break;
      }
    }
    head.lastIndex = Math.max(head.lastIndex, idx);

    let year: number | undefined;
    const yearRaw = rawFields["year"] || rawFields["date"];
    if (yearRaw) {
      const parsedYear = parseInt(yearRaw.replace(/\D/g, "").slice(0, 4), 10);
      if (!Number.isNaN(parsedYear)) {
        year = parsedYear;
      }
    }

    let month: number | undefined;
    if (rawFields["month"]) {
      const parsedMonth = parseInt(rawFields["month"].replace(/\D/g, ""), 10);
      if (!Number.isNaN(parsedMonth)) month = parsedMonth;
    }

    const authors = rawFields["author"] ? parseBibAuthors(rawFields["author"]) : [];
    const title = rawFields["title"];
    const journalOrPublisher =
      rawFields["journal"] ||
      rawFields["journaltitle"] ||
      rawFields["booktitle"] ||
      rawFields["publisher"] ||
      rawFields["school"] ||
      rawFields["institution"];

    let doi = rawFields["doi"];
    if (doi) {
      doi = doi
        .trim()
        .replace(/^https?:\/\/doi\.org\//i, "")
        .replace(/^doi:/i, "");
    }

    entries.push({
      key,
      entry_type: entryType,
      title: title ? cleanText(title) : undefined,
      authors: authors.length > 0 ? authors : undefined,
      year,
      month,
      journal_or_publisher: journalOrPublisher
        ? cleanText(journalOrPublisher)
        : undefined,
      volume: rawFields["volume"],
      number: rawFields["number"] || rawFields["issue"],
      pages: rawFields["pages"],
      doi,
      url: rawFields["url"],
      arxiv_id: rawFields["eprint"] || rawFields["arxiv"] || rawFields["arxivid"],
      abstract_text: rawFields["abstract"],
      raw_fields: Object.keys(rawFields).length > 0 ? rawFields : undefined,
    });
  }
  return entries;
}

function parseCslEntries(source: string, filename?: string): CitationEntry[] {
  let parsed: unknown;
  try {
    parsed = JSON.parse(source);
  } catch (err) {
    throw {
      file: filename,
      line: 1,
      column: 1,
      message: `CSL-JSON parse error: ${err}`,
    };
  }

  const items = Array.isArray(parsed)
    ? parsed
    : typeof parsed === "object" && parsed !== null
      ? [parsed]
      : [];
  const entries: CitationEntry[] = [];

  for (let idx = 0; idx < items.length; idx += 1) {
    const item = items[idx] as Record<string, unknown>;
    if (!item || typeof item !== "object") continue;

    const key = String(item["id"] ?? `csl_${idx}`);
    const entryType = String(item["type"] ?? "article");
    const title =
      typeof item["title"] === "string" ? cleanText(item["title"]) : undefined;

    const rawAuthors = item["author"];
    const authors: CitationAuthor[] = [];
    if (Array.isArray(rawAuthors)) {
      for (const a of rawAuthors) {
        if (typeof a === "object" && a !== null) {
          const family = typeof a.family === "string" ? a.family.trim() : undefined;
          const given = typeof a.given === "string" ? a.given.trim() : undefined;
          const name =
            typeof a.literal === "string"
              ? a.literal.trim()
              : [given, family].filter(Boolean).join(" ");
          if (name.length > 0) {
            authors.push({ name, family, given });
          }
        }
      }
    }

    let year: number | undefined;
    const issued = item["issued"] as { "date-parts"?: number[][] } | undefined;
    if (issued && Array.isArray(issued["date-parts"]) && issued["date-parts"][0]) {
      const y = issued["date-parts"][0][0];
      if (typeof y === "number") year = y;
    }

    const journalOrPublisher =
      typeof item["container-title"] === "string"
        ? item["container-title"]
        : typeof item["publisher"] === "string"
          ? item["publisher"]
          : undefined;

    const doi = typeof item["DOI"] === "string" ? item["DOI"] : undefined;
    const url = typeof item["URL"] === "string" ? item["URL"] : undefined;

    entries.push({
      key,
      entry_type: entryType,
      title,
      authors: authors.length > 0 ? authors : undefined,
      year,
      journal_or_publisher: journalOrPublisher
        ? cleanText(journalOrPublisher)
        : undefined,
      volume: typeof item["volume"] === "string" ? item["volume"] : undefined,
      number: typeof item["issue"] === "string" ? item["issue"] : undefined,
      pages: typeof item["page"] === "string" ? item["page"] : undefined,
      doi,
      url,
    });
  }

  return entries;
}

function formatApaAuthors(authors?: readonly CitationAuthor[]): string {
  if (!authors || authors.length === 0) return "";
  const formatted = authors.map((a) => {
    if (a.family && a.given) {
      const initials = a.given
        .split(/\s+/)
        .map((part) => (part[0] ? `${part[0]}.` : ""))
        .join(" ");
      return `${a.family}, ${initials}`.trim();
    }
    return a.name;
  });
  if (formatted.length === 1) return formatted[0] ?? "";
  if (formatted.length === 2) return `${formatted[0]} & ${formatted[1]}`;
  return `${formatted.slice(0, -1).join(", ")}, & ${formatted[formatted.length - 1]}`;
}

function formatNumericAuthors(authors?: readonly CitationAuthor[]): string {
  if (!authors || authors.length === 0) return "";
  const formatted = authors.map((a) => {
    if (a.family && a.given) {
      const initials = a.given
        .split(/\s+/)
        .map((part) => (part[0] ? `${part[0]}.` : ""))
        .join(" ");
      return `${initials} ${a.family}`.trim();
    }
    return a.name;
  });
  if (formatted.length === 1) return formatted[0] ?? "";
  if (formatted.length === 2) return `${formatted[0]} and ${formatted[1]}`;
  return `${formatted.slice(0, -1).join(", ")}, and ${formatted[formatted.length - 1]}`;
}

export function formatApaEntry(entry: CitationEntry): string {
  const parts: string[] = [];
  const authors = formatApaAuthors(entry.authors);
  if (authors) parts.push(authors);

  const yearStr = entry.year !== undefined ? `(${entry.year}).` : "(n.d.).";
  parts.push(yearStr);

  if (entry.title) {
    let t = entry.title.trim();
    if (!t.endsWith(".") && !t.endsWith("?") && !t.endsWith("!")) {
      t += ".";
    }
    parts.push(t);
  }

  if (entry.journal_or_publisher) {
    let s = entry.journal_or_publisher;
    if (entry.volume) {
      s += `, ${entry.volume}`;
      if (entry.number) s += `(${entry.number})`;
    } else if (entry.number) {
      s += `, No. ${entry.number}`;
    }
    if (entry.pages) s += `, ${entry.pages}`;
    if (!s.endsWith(".")) s += ".";
    parts.push(s);
  }

  if (entry.doi) {
    parts.push(`https://doi.org/${entry.doi}`);
  } else if (entry.url) {
    parts.push(entry.url);
  }

  return parts.join(" ");
}

export function formatNumericEntry(entry: CitationEntry): string {
  const parts: string[] = [];
  const authors = formatNumericAuthors(entry.authors);
  if (authors) parts.push(`${authors},`);

  if (entry.title) {
    const t = entry.title.trim().replace(/\.$/, "");
    parts.push(`"${t},"`);
  }

  if (entry.journal_or_publisher) {
    parts.push(entry.journal_or_publisher);
  }
  if (entry.volume) {
    parts.push(`vol. ${entry.volume}`);
  }
  if (entry.number) {
    parts.push(`no. ${entry.number}`);
  }
  if (entry.pages) {
    parts.push(`pp. ${entry.pages}`);
  }
  if (entry.year !== undefined) {
    parts.push(String(entry.year));
  }

  let base = parts.join(", ");
  if (!base.endsWith(".")) base += ".";

  if (entry.doi) {
    base += ` https://doi.org/${entry.doi}`;
  } else if (entry.url) {
    base += ` ${entry.url}`;
  }

  return `[1] ${base}`;
}

export function extractKeysFromSource(
  source: string,
  syntax: "typst" | "latex" | "markdown",
): string[] {
  const keys: string[] = [];
  const seen = new Set<string>();

  const add = (k: string) => {
    const cleanKey = k.trim();
    if (cleanKey.length > 0 && !seen.has(cleanKey)) {
      seen.add(cleanKey);
      keys.push(cleanKey);
    }
  };

  if (syntax === "typst") {
    const regex = /(?:^|[^\w@])@([A-Za-z0-9_:-]+)/g;
    let m = regex.exec(source);
    while (m !== null) {
      if (m[1] && !/^\d+$/.test(m[1])) {
        add(m[1]);
      }
      m = regex.exec(source);
    }
  } else if (syntax === "latex") {
    const regex =
      /\\(?:cite[a-z*]*|autocite|textcite|parencite|citet|citep)\{([^}]+)\}/gi;
    let m = regex.exec(source);
    while (m !== null) {
      if (m[1]) {
        for (const k of m[1].split(",")) {
          add(k);
        }
      }
      m = regex.exec(source);
    }
  } else {
    // Markdown
    const regex = /\[?@([A-Za-z0-9_:-]+)\]?/g;
    let m = regex.exec(source);
    while (m !== null) {
      if (m[1] && !/^\d+$/.test(m[1])) {
        add(m[1]);
      }
      m = regex.exec(source);
    }
  }

  return keys;
}

/** The styles `format.rs` knows (`is_supported_style`). */
function isSupportedStyle(style: string): boolean {
  return ["apa", "apa-like", "numeric", "ieee"].includes(style.trim().toLowerCase());
}

export function citationsPlugin(host: MockProjectHost): MockPluginFactory {
  const index = new Map<string, CitationEntry>();
  /** `citations.style`, applied by `citations.configure`. */
  let configuredStyle = "apa";

  return (_emit): MockPlugin => {
    return {
      name: "citations",
      commands: COMMANDS,
      handlers: {
        "citations.load": (args) => {
          let paths: string[] = [];
          if (Array.isArray(args["paths"])) {
            paths = args["paths"].filter((p): p is string => typeof p === "string");
          } else if (typeof args["path"] === "string") {
            paths = [args["path"]];
          } else {
            throw {
              message: "either `paths` (string[]) or `path` (string) is required",
            };
          }

          let loaded = 0;
          const errors: CitationParseDiagnostic[] = [];
          const currentFs = host.current;

          for (const p of paths) {
            let content: string | undefined;

            if (currentFs) {
              const rootPrefix = `${currentFs.root}/`;
              const rel = p.startsWith(rootPrefix) ? p.slice(rootPrefix.length) : p;
              const direct = currentFs.files.get(rel) ?? currentFs.files.get(p);
              if (typeof direct === "string") {
                content = direct;
              } else if (direct instanceof Uint8Array) {
                content = new TextDecoder().decode(direct);
              }
            }

            if (content === undefined) {
              errors.push({
                file: p,
                line: 1,
                column: 1,
                message: `failed to read file \`${p}\`: file not found`,
              });
              continue;
            }

            try {
              let parsed: CitationEntry[];
              const isJson = p.endsWith(".json") || p.endsWith(".csl");
              if (isJson) {
                parsed = parseCslEntries(content, p);
              } else {
                parsed = parseBibtexEntries(content);
              }

              for (const entry of parsed) {
                index.set(entry.key, entry);
                loaded += 1;
              }
            } catch (err: unknown) {
              const diag = err as CitationParseDiagnostic;
              errors.push({
                file: diag.file ?? p,
                line: diag.line ?? 1,
                column: diag.column ?? 1,
                message: diag.message ?? String(err),
              });
            }
          }

          const response: CitationLoadResponse = {
            loaded,
            total: index.size,
            errors,
          };
          return response;
        },

        "citations.search": (args) => {
          const query = str(args, "query").trim().toLowerCase();
          const limit =
            typeof args["limit"] === "number" ? Math.max(0, args["limit"]) : 100;

          const allEntries = [...index.values()];
          if (query === "") {
            const results = allEntries.slice(0, limit).map((entry) => ({
              key: entry.key,
              score: 1.0,
              entry,
            }));
            const res: CitationSearchResponse = { results };
            return res;
          }

          const queryWords = query.split(/\s+/).filter((w) => w.length > 0);
          const scored: { key: string; score: number; entry: CitationEntry }[] = [];

          for (const entry of allEntries) {
            const authorsStr = (entry.authors ?? []).map((a) => a.name).join(" ");
            const haystack =
              `${entry.key} ${entry.title ?? ""} ${authorsStr} ${entry.year ?? ""} ${entry.journal_or_publisher ?? ""} ${entry.doi ?? ""}`.toLowerCase();

            let matchedWords = 0;
            let score = 0;

            for (const word of queryWords) {
              if (haystack.includes(word)) {
                matchedWords += 1;
                if (entry.key.toLowerCase().includes(word)) score += 5;
                if (entry.title?.toLowerCase().includes(word)) score += 3;
                if (authorsStr.toLowerCase().includes(word)) score += 2;
                score += 1;
              }
            }

            if (matchedWords === queryWords.length) {
              scored.push({ key: entry.key, score, entry });
            }
          }

          scored.sort((a, b) => b.score - a.score || a.key.localeCompare(b.key));
          const response: CitationSearchResponse = {
            results: scored.slice(0, limit),
          };
          return response;
        },

        "citations.get": (args) => {
          const key = str(args, "key");
          const entry = index.get(key);
          if (!entry) {
            throw { message: `entry \`${key}\` not found` };
          }
          return entry;
        },

        // Like `plugin.rs`: all-or-nothing, unknown ids are ignored.
        "citations.configure": (args) => {
          const values = args["values"];
          if (typeof values !== "object" || values === null || Array.isArray(values)) {
            throw { message: "bad arguments: `values` must be an object" };
          }
          const given = (values as Record<string, unknown>)["citations.style"];
          if (given !== undefined) {
            if (typeof given !== "string" || !isSupportedStyle(given)) {
              throw {
                message: `\`citations.style\`: unsupported citation style \`${String(given)}\``,
              };
            }
            configuredStyle = given;
          }
          return { "citations.style": configuredStyle };
        },

        "citations.format": (args) => {
          const key = str(args, "key");
          const style = optString(args, "style") ?? configuredStyle;
          const entry = index.get(key);
          if (!entry) {
            throw { message: `entry \`${key}\` not found` };
          }

          let formatted = "";
          if (style.toLowerCase() === "numeric" || style.toLowerCase() === "ieee") {
            formatted = formatNumericEntry(entry);
          } else {
            formatted = formatApaEntry(entry);
          }

          const response: CitationFormatResponse = {
            key,
            style,
            formatted,
          };
          return response;
        },

        "citations.extract-keys": (args) => {
          const source = str(args, "source");
          const syntax = str(args, "syntax") as "typst" | "latex" | "markdown";
          const keys = extractKeysFromSource(source, syntax);
          return { keys };
        },

        "citations.verify": (args) => {
          const key = str(args, "key");
          const entry = index.get(key);
          if (!entry) {
            throw { message: `entry \`${key}\` not found` };
          }

          // Mock remote registry verification
          if (entry.key.includes("mismatch") || entry.title?.includes("Mismatch")) {
            const differences: CitationDifference[] = [
              {
                field: "year",
                local: String(entry.year ?? 2020),
                remote: String((entry.year ?? 2020) - 1),
              },
            ];
            const result: CitationVerificationResult = {
              status: "mismatch",
              matched: {
                ...entry,
                year: (entry.year ?? 2020) - 1,
              },
              differences,
              message: null,
            };
            return result;
          }

          if (
            entry.key.includes("notfound") ||
            (!entry.doi && !entry.arxiv_id && !entry.title)
          ) {
            const result: CitationVerificationResult = {
              status: "not_found",
              matched: null,
              differences: [],
              message:
                "no identifier (DOI, arXiv ID) or title matched in remote registries",
            };
            return result;
          }

          if (entry.key.includes("error")) {
            const result: CitationVerificationResult = {
              status: "error",
              matched: null,
              differences: [],
              message: "network timeout connecting to api.crossref.org",
            };
            return result;
          }

          // Default: verified
          const result: CitationVerificationResult = {
            status: "verified",
            matched: entry,
            differences: [],
            message: null,
          };
          return result;
        },
      },
    };
  };
}
