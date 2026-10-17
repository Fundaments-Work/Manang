/**
 * Presentation Studio — DESIGN.md §8.
 *
 * A deck is a list of slides parsed from the manuscript's slide region. The
 * shell owns navigation and layout; the engine renders the slide bodies.
 */

export interface Slide {
  readonly id: string;
  /** Numbered section label, e.g. "03". */
  readonly ordinal: string;
  readonly title: string;
  readonly kicker: string;
  readonly body: string;
  /** Rendered equation shown on the slide, if any. */
  readonly math?: string;
  readonly citation?: string;
}

const DECK: readonly Slide[] = [
  {
    id: "s1",
    ordinal: "01",
    kicker: "QUANTUM THERMODYNAMICS",
    title: "Landauer Bound in Quantum Regimes",
    body: "Minimal heat dissipated per erased bit of physical entropy, extended to coherently controlled erasure.",
    math: "ΔQ_diss ≥ k_B · T · ln 2 + D(ρ ‖ ρ_eq)",
    citation: "[1] Landauer, R. (1961) IBM J. Res. Dev. 5, 215",
  },
  {
    id: "s2",
    ordinal: "02",
    kicker: "QUANTUM THERMODYNAMICS · §1",
    title: "Irreversibility Without Heat",
    body: "Under unitary coupling the entropy production rate is bounded below even when the bath is cold.",
    math: "Σ ≥ k_B D(ρ ‖ ρ_eq)",
    citation: "[2] Bérut, A. et al. (2012) PRL 108, 100602",
  },
  {
    id: "s3",
    ordinal: "03",
    kicker: "QUANTUM THERMODYNAMICS · §2",
    title: "Finite-Time Operation",
    body: "The bound survives finite-time protocols, where quasistatic assumptions fail.",
    math: "ΔS_irr ≥ ∫₀ᵗ δQ / T + k_B D",
    citation: "[1] Landauer, R. (1961) IBM J. Res. Dev. 5, 215",
  },
  {
    id: "s4",
    ordinal: "04",
    kicker: "QUANTUM THERMODYNAMICS · §3",
    title: "Numerical Simulation",
    body: "Monte Carlo trajectories of a driven qubit confirm the analytic bound across 10⁴ cycles.",
    citation: "[3] Lloyd, S. (2008) Phys. Rev. A 78, 032334",
  },
  {
    id: "s5",
    ordinal: "05",
    kicker: "QUANTUM THERMODYNAMICS · §4",
    title: "Coherence as a Work Resource",
    body: "Finite coherence retains work capacity under thermal dissipation.",
    citation: "[2] Bérut, A. et al. (2012) PRL 108, 100602",
  },
  {
    id: "s6",
    ordinal: "06",
    kicker: "QUANTUM THERMODYNAMICS · §5",
    title: "Open Questions",
    body: "Measurement-only erasure, non-Markovian baths, and the cost of quantum corrections.",
  },
];

export class SlidesState {
  readonly slides: readonly Slide[] = DECK;

  index = $state(2);

  readonly current: Slide | undefined = $derived(this.slides[this.index]);

  readonly count: number = $derived(this.slides.length);

  /** 1-based, as shown in the slide frame's corner. */
  readonly displayNumber: number = $derived(this.index + 1);

  readonly atStart: boolean = $derived(this.index === 0);

  readonly atEnd: boolean = $derived(this.index >= this.slides.length - 1);

  goTo(index: number): void {
    this.index = Math.max(0, Math.min(index, this.slides.length - 1));
  }

  next(): void {
    this.goTo(this.index + 1);
  }

  previous(): void {
    this.goTo(this.index - 1);
  }
}

export const deck = new SlidesState();
