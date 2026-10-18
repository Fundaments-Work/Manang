/**
 * What the composer shows and sends besides the author's words: one quiet line ("main.typ,
 * selection (38 words)"), the slash command typed at the start of the message, and @-mentions.
 *
 * The token figure is a local approximation (about 3.5 characters per token); the provider's own
 * count is the truth and the cost is always labelled an estimate.
 */

import type { Mention, SlashCommand } from "./types";

/** About 3.5 characters per token, rounded up (the Rust side uses the same rule). */
export function estimateTokens(text: string): number {
  const chars = Array.from(text).length;
  return Math.ceil((chars * 2) / 7);
}

export function wordCount(text: string): number {
  const words = text.trim().split(/\s+/);
  return words.length === 1 && words[0] === "" ? 0 : words.length;
}

export function mentionLabel(mention: Mention): string {
  switch (mention.kind) {
    case "file":
      return `@${mention.path}`;
    case "problems":
      return "@problems";
    case "figure":
      return `@figure${mention.path === undefined ? "" : ` ${mention.path}`}`;
  }
}

/** The line under the composer: what travels with the next message. */
export function contextLine(parts: {
  readonly path?: string | undefined;
  readonly selectionWords?: number | undefined;
  readonly mentions: readonly Mention[];
}): string {
  const out: string[] = [];
  if (parts.path !== undefined) out.push(parts.path);
  if (parts.selectionWords !== undefined && parts.selectionWords > 0) {
    out.push(
      `selection (${parts.selectionWords} word${parts.selectionWords === 1 ? "" : "s"})`,
    );
  }
  for (const m of parts.mentions) out.push(mentionLabel(m));
  return out.join(", ");
}

/**
 * Splits `/name rest` into the command and its arguments. A message that starts with an unknown
 * `/word` is sent as plain text.
 */
export function parseSlash(
  input: string,
  commands: readonly SlashCommand[],
): { readonly command: string | null; readonly prompt: string } {
  const m = /^\/([a-z0-9_-]+)(?:\s+([\s\S]*))?$/.exec(input.trim());
  if (m === null) return { command: null, prompt: input.trim() };
  const name = m[1] ?? "";
  if (!commands.some((c) => c.name === name)) return { command: null, prompt: input.trim() };
  return { command: name, prompt: (m[2] ?? "").trim() };
}

/** The commands the menu offers while the author is still typing the name. */
export function slashMatches(
  input: string,
  commands: readonly SlashCommand[],
): readonly SlashCommand[] {
  const m = /^\/([a-z0-9_-]*)$/.exec(input);
  if (m === null) return [];
  const query = m[1] ?? "";
  return commands.filter((c) => c.name.startsWith(query));
}

/** `1.2k` style size for token figures. */
export function tokenLabel(tokens: number): string {
  return tokens < 1000 ? `${tokens}` : `${(tokens / 1000).toFixed(1)}k`;
}
