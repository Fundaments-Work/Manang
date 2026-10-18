/** "What's new": the newest section of CHANGELOG.md as a short list. */

export interface Changes {
  /** The section's heading, e.g. `Unreleased` or `0.1.0 - 2026-11-01`. */
  readonly heading: string;
  readonly items: readonly string[];
}

/** Drop the Markdown marks a plain list does not need. */
function plain(text: string): string {
  return text
    .replace(/\*\*(.+?)\*\*/g, "$1")
    .replace(/`(.+?)`/g, "$1")
    .replace(/\[(.+?)\]\(.+?\)/g, "$1")
    .trim();
}

/**
 * The bullets of the first `## ` section (the newest), at most `limit` of them,
 * each cut at its first sentence so the list stays scannable.
 */
export function latestChanges(markdown: string, limit = 12): Changes {
  const lines = markdown.split("\n");
  const start = lines.findIndex((line) => line.startsWith("## "));
  if (start === -1) return { heading: "", items: [] };
  const heading = plain(lines[start]?.slice(3).replace(/[[\]]/g, "") ?? "");
  const items: string[] = [];
  for (const line of lines.slice(start + 1)) {
    if (line.startsWith("## ")) break;
    if (!line.startsWith("- ") || items.length >= limit) continue;
    const text = plain(line.slice(2));
    const sentence = /^(.+?[.!?])(\s|$)/.exec(text)?.[1] ?? text;
    items.push(sentence);
  }
  return { heading, items };
}
