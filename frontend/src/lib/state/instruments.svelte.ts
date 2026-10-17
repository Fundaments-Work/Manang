/**
 * Instrument shelf — DESIGN.md §6.
 *
 * Every engine in Manang is an isolated WebAssembly instrument, so the shelf
 * lists capabilities alongside names. Checking an instrument enables its
 * sandbox instantly; the permission summary is always one hover away, because a
 * plugin manager that hides what a plugin may touch is just a trust prompt with
 * nicer typography.
 */

export type InstrumentState = "enabled" | "sandboxed";

export interface Capabilities {
  /** Buffer access the instrument requested. */
  readonly buffer: readonly string[];
  /** Network origins, empty for none. */
  readonly network: readonly string[];
  /** Filesystem scopes. */
  readonly filesystem: readonly string[];
}

export interface Instrument {
  readonly id: string;
  readonly name: string;
  readonly description: string;
  readonly version: string;
  readonly kind:
    "Core Instrument" | "Drawing Plugin" | "Graphic Plugin" | "Presentation Plugin";
  readonly capabilities: Capabilities;
  /** Core instruments cannot be switched off. */
  readonly essential?: boolean;
}

const SEED: readonly Instrument[] = [
  {
    id: "typst",
    name: "Typst Compiler Engine",
    description: "Native in-memory compilation",
    version: "0.12.0",
    kind: "Core Instrument",
    essential: true,
    capabilities: {
      buffer: ["read:active", "write:active"],
      network: [],
      filesystem: [],
    },
  },
  {
    id: "ink",
    name: "Vector Ink Engine",
    description: "Infinite vector sketch pad",
    version: "1.4.0",
    kind: "Drawing Plugin",
    capabilities: { buffer: ["write:figures"], network: [], filesystem: ["figures/"] },
  },
  {
    id: "cetz",
    name: "CeTZ Mathematical Vectors",
    description: "Precise coordinate geometry",
    version: "0.3.1",
    kind: "Graphic Plugin",
    capabilities: { buffer: ["read:active"], network: [], filesystem: ["figures/"] },
  },
  {
    id: "touying",
    name: "Touying Slide Decks",
    description: "16:9 presentation studio",
    version: "0.5.2",
    kind: "Presentation Plugin",
    capabilities: {
      buffer: ["read:active", "write:slides"],
      network: [],
      filesystem: ["slides/"],
    },
  },
  {
    id: "vale",
    name: "Vale Prose Linter",
    description: "Prose linting in the margin",
    version: "3.9.0",
    kind: "Graphic Plugin",
    capabilities: {
      buffer: ["read:active"],
      network: [],
      filesystem: [".vale/styles"],
    },
  },
  {
    id: "crossref",
    name: "Scholarly Verifier",
    description: "DOI and title resolution",
    version: "0.2.0",
    kind: "Graphic Plugin",
    capabilities: {
      buffer: ["read:bibliography"],
      network: ["api.crossref.org", "api.openalex.org"],
      filesystem: [],
    },
  },
];

export class InstrumentShelfState {
  readonly instruments: readonly Instrument[] = $state(structuredClone(SEED));

  readonly states: Record<string, InstrumentState> = $state(
    Object.fromEntries(SEED.map((instrument) => [instrument.id, "enabled" as const])),
  );

  readonly enabledCount: number = $derived(
    this.instruments.filter((instrument) => this.states[instrument.id] === "enabled")
      .length,
  );

  isEnabled(id: string): boolean {
    return this.states[id] === "enabled";
  }

  /**
   * Essential instruments are core engines, not extensions; disabling them
   * would break the app rather than remove a capability, so the toggle is a
   * no-op for them.
   */
  toggle(id: string): void {
    const instrument = this.instruments.find((entry) => entry.id === id);
    if (instrument?.essential === true) return;
    this.states[id] = this.states[id] === "enabled" ? "sandboxed" : "enabled";
  }
}

export const instruments = new InstrumentShelfState();

/** Compact permission summary, e.g. `Buffer: read | Net: none | FS: figures/`. */
export function capabilitySummary(capabilities: Capabilities): string {
  const buffer =
    capabilities.buffer.length === 0
      ? "none"
      : capabilities.buffer.some((entry) => entry.includes("write"))
        ? "read | write"
        : "read";
  const net =
    capabilities.network.length === 0 ? "none" : capabilities.network.join(", ");
  const fs =
    capabilities.filesystem.length === 0 ? "none" : capabilities.filesystem.join(", ");
  return `Buffer: ${buffer}  |  Net: ${net}  |  FS: ${fs}`;
}
