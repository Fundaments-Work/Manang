/**
 * Compile state: the result of the last `typst.compile`, its problems and the
 * Output log.
 *
 * Compiling is debounced while typing (250 ms) and always sends the unsaved
 * text of every open document, so the preview and Problems reflect what is on
 * screen rather than what is on disk. Only Typst has a compiler today; other
 * engines report "unsupported" instead of pretending.
 */

import { kernel, errorMessage } from "$lib/kernel";
import { typstApi } from "$lib/kernel/api";
import { engineOf } from "$lib/workbench/paths";
import {
  countProblems,
  fromReport,
  type Counts,
  type Problem,
} from "$lib/workbench/problems";

import { documents } from "./documents.svelte";
import { tabs } from "./tabs.svelte";
import { workspace } from "./workspace.svelte";

export type BuildStatus =
  | "idle"
  | "compiling"
  | "ok"
  | "failed"
  /** The open document's engine has no compiler plugin yet. */
  | "unsupported"
  /** The kernel offers no `typst.compile`. */
  | "unavailable";

export interface LogLine {
  readonly id: number;
  readonly time: string;
  readonly text: string;
  readonly level: "info" | "error";
}

const DEBOUNCE_MS = 250;
const MAX_LOG = 200;
const api = typstApi(kernel);

function clock(): string {
  return new Date().toLocaleTimeString([], { hour12: false });
}

export class BuildState {
  status = $state<BuildStatus>("idle");
  problems = $state.raw<readonly Problem[]>([]);
  durationMs = $state(0);
  pages = $state(0);
  /** Which file the last result describes. */
  target = $state("");
  log = $state.raw<readonly LogLine[]>([]);
  /** Bumped after each successful compile so the preview can re-render. */
  generation = $state(0);

  readonly counts: Counts = $derived(countProblems(this.problems));

  #timer: ReturnType<typeof setTimeout> | undefined;
  #run = 0;
  #logId = 0;
  #available: boolean | undefined;

  /**
   * The file the preview shows. A Typst document previews itself; a LaTeX or
   * Markdown document has no compiler, so there is nothing to show; anything
   * else (a bibliography, a figure, no tab) falls back to the project's main file.
   */
  previewTarget(): string | null {
    const active = tabs.active;
    if (active?.kind === "document") {
      const engine = engineOf(active.path);
      if (engine === "typst") return active.path;
      if (engine !== null) return null;
    }
    const project = workspace.project;
    return project !== null && project.engine === "typst" ? project.main : null;
  }

  /** Re-compile once typing settles. */
  schedule(): void {
    clearTimeout(this.#timer);
    this.#timer = setTimeout(() => void this.compile(), DEBOUNCE_MS);
  }

  async #hasCompiler(): Promise<boolean> {
    if (this.#available === undefined) {
      try {
        this.#available = (await kernel.commands()).some(
          (c) => c.id === "typst.compile",
        );
      } catch {
        this.#available = false;
      }
    }
    return this.#available;
  }

  async compile(): Promise<void> {
    clearTimeout(this.#timer);
    const run = ++this.#run;

    if (workspace.project === null) {
      this.#reset("idle");
      return;
    }
    const main = this.previewTarget();
    if (main === null) {
      this.#reset("unsupported");
      return;
    }
    if (!(await this.#hasCompiler())) {
      this.#reset("unavailable");
      return;
    }

    this.status = "compiling";
    try {
      const report = await api.compile({
        main,
        root: workspace.project.root,
        sources: documents.dirtySources(),
      });
      if (run !== this.#run) return;
      this.target = main;
      this.problems = fromReport(report);
      this.durationMs = report.duration_ms;
      this.pages = report.pages;
      if (report.ok) {
        this.status = "ok";
        this.generation += 1;
        this.#write(
          `Compiled ${main} in ${formatMs(report.duration_ms)} (${report.pages} ${report.pages === 1 ? "page" : "pages"}).`,
          "info",
        );
      } else {
        this.status = "failed";
        const { errors } = countProblems(this.problems);
        this.#write(
          `${main} has ${errors} ${errors === 1 ? "error" : "errors"}; showing the last good pages.`,
          "error",
        );
      }
    } catch (error) {
      if (run !== this.#run) return;
      this.status = "failed";
      this.problems = [];
      this.#write(`Compile did not run: ${errorMessage(error)}`, "error");
    }
  }

  #reset(status: BuildStatus): void {
    this.status = status;
    this.problems = [];
    this.pages = 0;
    this.target = "";
  }

  #write(text: string, level: LogLine["level"]): void {
    this.#logId += 1;
    this.log = [...this.log, { id: this.#logId, time: clock(), text, level }].slice(
      -MAX_LOG,
    );
  }

  clearLog(): void {
    this.log = [];
  }
}

export function formatMs(ms: number): string {
  return `${Math.round(ms)} ms`;
}

export const build = new BuildState();
