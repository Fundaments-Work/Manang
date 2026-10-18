/**
 * Context chips: what the tray shows and what `agent.start` sends. Nothing else is sent.
 *
 * The token figure is a local approximation (about 3.5 characters per token) for the live
 * meter; the provider's own count is the truth and the cost is always labelled an estimate.
 */

import type { ChipInput, ChipKind, TrayChip, WireDiagnostic } from "./types";

/** About 3.5 characters per token, rounded up (the Rust side uses the same rule). */
export function estimateTokens(text: string): number {
  const chars = Array.from(text).length;
  return Math.ceil((chars * 2) / 7);
}

/** Most characters the host sends per kind; larger chips are cut (and say so). */
const CAPS: Readonly<Record<ChipKind, number>> = {
  selection: 40_000,
  file: 80_000,
  diagnostics: 8_000,
  outline: 6_000,
  "bibliography-keys": 8_000,
  figure: 12_000,
  pinned: 20_000,
};

export function chipText(chip: ChipInput): string {
  if (chip.text !== undefined) return chip.text;
  return (chip.diagnostics ?? []).map((d) => `${d.message}\n${d.context ?? ""}`).join("\n");
}

/** The tokens of what would be sent for this chip (capped like the host does). */
export function chipTokens(chip: ChipInput): number {
  const text = Array.from(chipText(chip)).slice(0, CAPS[chip.kind]).join("");
  return estimateTokens(text);
}

export function totalTokens(chips: readonly TrayChip[]): number {
  return chips.reduce((sum, c) => sum + c.tokens, 0);
}

function label(kind: ChipKind, path: string | undefined): string {
  const file = path ?? "document";
  switch (kind) {
    case "selection":
      return `Selection in ${file}`;
    case "file":
      return file;
    case "diagnostics":
      return "Problems";
    case "outline":
      return `Outline of ${file}`;
    case "bibliography-keys":
      return "Bibliography keys";
    case "figure":
      return `Figure ${file}`;
    case "pinned":
      return "Pinned note";
  }
}

let counter = 0;

/** A tray chip from an input; ids are unique per page load. */
export function trayChip(input: ChipInput): TrayChip {
  counter += 1;
  return {
    ...input,
    id: input.id ?? `${input.kind}-${counter}`,
    label: input.label ?? label(input.kind, input.path),
    tokens: chipTokens(input),
  };
}

export function selectionChip(path: string, from: number, to: number, text: string): TrayChip {
  return trayChip({ kind: "selection", path, range: [from, to], text });
}

export function fileChip(path: string, text: string): TrayChip {
  return trayChip({ kind: "file", path, text });
}

export function diagnosticsChip(diagnostics: readonly WireDiagnostic[]): TrayChip {
  return trayChip({ kind: "diagnostics", diagnostics });
}

/** Strips the tray-only fields: what goes over the wire. */
export function toWire(chip: TrayChip): ChipInput {
  const { tokens: _tokens, ...rest } = chip;
  return rest;
}

/**
 * The tray after a new request attaches `incoming`: pinned chips stay, a chip of the same
 * kind and path is replaced (a new selection replaces the old one), the rest are kept.
 */
export function mergeTray(current: readonly TrayChip[], incoming: readonly TrayChip[]): TrayChip[] {
  const kept = current.filter(
    (c) => !incoming.some((n) => n.kind === c.kind && n.kind !== "pinned" && n.path === c.path),
  );
  return [...kept, ...incoming];
}

/** Removes one chip. */
export function removeChip(tray: readonly TrayChip[], id: string): TrayChip[] {
  return tray.filter((c) => c.id !== id);
}

/** Pins or unpins a chip: pinned chips survive `clearUnpinned`. */
export function togglePin(tray: readonly TrayChip[], id: string): TrayChip[] {
  return tray.map((c) => (c.id === id ? { ...c, pinned: c.pinned !== true } : c));
}

export function clearUnpinned(tray: readonly TrayChip[]): TrayChip[] {
  return tray.filter((c) => c.pinned === true);
}

/** `1.2k` style size for the tray. */
export function tokenLabel(tokens: number): string {
  return tokens < 1000 ? `${tokens}` : `${(tokens / 1000).toFixed(1)}k`;
}
