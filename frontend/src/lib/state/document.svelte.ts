/**
 * The active manuscript.
 *
 * AGENTS.md puts the document buffer in Rust behind ropey; this store is the
 * shell's mirror of it, kept deliberately minimal — text, language, and the
 * diagnostics derived from them. Anything expensive (parsing, compiling) is
 * the engine's job and arrives as an event.
 */

import { analyse, type Diagnostic } from "$lib/editor/diagnostics";

export type DocumentLanguage = "typst" | "latex" | "markdown";

export interface DocumentMeta {
  readonly id: string;
  readonly path: string;
  readonly language: DocumentLanguage;
}

const SAMPLE_TYPST = `= Quantum Thermodynamic Bounds

Under unitary coupling to a heat reservoir at inverse temperature
$beta = (k_B T)^(-1)$, the irreversible entropy production rate
$Sigma$ satisfies the finite-time lower bound:

$$ Delta S_irr >= integral_0^t (delta Q)/T $$

This generalizes Landauer to finite-time operation, as shown in
@landauer1961.

$ Delta S_irr $

#figure(image("figures/state_lattice.svg", width: 80%))
`;

export class DocumentState {
  readonly meta = $state<DocumentMeta>({
    id: "quantum_entropy",
    path: "papers/quantum_entropy.typ",
    language: "typst",
  });

  text = $state(SAMPLE_TYPST);

  #knownFigures: string[] = $state([]);

  /** Known figure paths, used to resolve `#image(…)` references. */
  setKnownFigures(paths: readonly string[]): void {
    this.#knownFigures = [...paths];
  }

  /**
   * Recomputed from `text` on every access.
   *
   * Deriving rather than storing means the gutter can never drift from the text
   * it annotates, which is the failure mode that makes editors feel untrustworthy.
   */
  readonly diagnostics: Diagnostic[] = $derived(
    analyse(this.text, { knownFigures: this.#knownFigures }),
  );

  readonly wordCount: number = $derived(
    this.text.split(/\s+/).filter((word) => word.length > 0).length,
  );

  readonly lineCount: number = $derived(this.text.split("\n").length);

  replace(next: string): void {
    this.text = next;
  }
}

export const document = new DocumentState();
