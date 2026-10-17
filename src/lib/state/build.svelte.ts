/**
 * Compile state: the result of the last compile (Typst, LaTeX or Markdown), its
 * problems and the Output log.
 *
 * One scheduler per build, shared by every engine:
 *  - Typst and Markdown recompile 250 ms after typing stops, from the unsaved
 *    text of every open document.
 *  - LaTeX compiles what is on disk (the plugin reads the project), so it runs
 *    shortly after a save, or when files change outside the editor, never per
 *    keystroke.
 *  - A run in flight is never started twice; a newer request runs once after it,
 *    and a result that is no longer the newest is dropped.
 *  - A compile whose inputs equal the last good one is skipped.
 */

import { kernel, errorMessage } from "$lib/kernel";
import { latexApi, markdownApi, typstApi } from "$lib/kernel/api";
import type { Engine } from "$lib/kernel/types";
import { join } from "$lib/workbench/paths";
import {
  countProblems,
  explainLatexFailure,
  fromLatexReport,
  fromMarkdown,
  fromReport,
  type Counts,
  type EngineFailure,
  type Problem,
} from "$lib/workbench/problems";
import { createScheduler } from "$lib/workbench/scheduler";
import { inputKey, resolveTarget, type Target } from "$lib/workbench/target";

import { documents } from "./documents.svelte";
import { tabs } from "./tabs.svelte";
import { workspace } from "./workspace.svelte";

export type BuildStatus =
  | "idle"
  | "compiling"
  | "ok"
  | "failed"
  /** The open file has nothing to compile. */
  | "unsupported"
  /** The kernel offers no compile command for the engine. */
  | "unavailable";

export interface LogLine {
  readonly id: number;
  readonly time: string;
  readonly text: string;
  readonly level: "info" | "error";
}

/** Why a compile was asked for; decides how long to wait. */
export type BuildReason = "edit" | "switch" | "disk";

/** Typst and Markdown follow typing; LaTeX waits for the save to reach disk. */
export const DEBOUNCE_MS: Readonly<Record<Engine, number>> = {
  typst: 250,
  markdown: 250,
  latex: 400,
};
const MAX_LOG = 200;
const typst = typstApi(kernel);
const latex = latexApi(kernel);
const markdown = markdownApi(kernel);

const COMPILE_COMMAND: Record<Engine, string> = {
  typst: "typst.compile",
  latex: "latex.compile",
  markdown: "markdown.render-html",
};

function clock(): string {
  return new Date().toLocaleTimeString([], { hour12: false });
}

export class BuildState {
  status = $state<BuildStatus>("idle");
  problems = $state.raw<readonly Problem[]>([]);
  durationMs = $state(0);
  pages = $state(0);
  /** Which file the last result describes (project-relative). */
  target = $state("");
  /** The engine of the last result; decides which viewer the preview uses. */
  engine = $state<Engine | null>(null);
  /** True when the active file is compiled through the project's main file. */
  viaMain = $state(false);
  /** Rendered, already sanitised HTML of the last Markdown preview. */
  html = $state("");
  /** The engine could not run at all (missing worker, no TeX bundle). */
  failure = $state.raw<EngineFailure | null>(null);
  log = $state.raw<readonly LogLine[]>([]);
  /** Bumped after each successful compile so the preview can re-render. */
  generation = $state(0);

  readonly counts: Counts = $derived(countProblems(this.problems));
  /** One line for the status bar. */
  readonly summary: string = $derived(statusText(this));

  #run = 0;
  #logId = 0;
  #available: Record<string, boolean> = {};
  #lastKey = "";
  #disk = 0;
  #network = false;
  #mainText: { path: string; disk: number; text: string } | null = null;
  readonly #scheduler = createScheduler(() => this.compile(), DEBOUNCE_MS.typst);

  /** The file the preview shows, or null when nothing previews. */
  previewTarget(): string | null {
    return this.#resolve()?.main ?? null;
  }

  #resolve(): Target | null {
    const active = tabs.active;
    const path = active?.kind === "document" ? active.path : null;
    const project = workspace.project;
    const main = project?.main ?? "";
    const mainSession = documents.all().find((s) => s.path === main);
    const cached = this.#mainText;
    const mainText =
      mainSession?.text() ??
      (cached !== null && cached.path === main && cached.disk === this.#disk
        ? cached.text
        : null);
    const activeSession =
      path === null ? undefined : documents.all().find((s) => s.path === path);
    return resolveTarget({
      active: path,
      project: project === null ? null : { engine: project.engine, main },
      mainText,
      activeText: activeSession?.text() ?? null,
    });
  }

  /** The main file's text is needed to tell chapters from standalone files. */
  async #loadMainText(): Promise<void> {
    const project = workspace.project;
    if (project === null || documents.all().some((s) => s.path === project.main))
      return;
    const cached = this.#mainText;
    if (cached?.path === project.main && cached.disk === this.#disk) return;
    try {
      const text = await workspace.readFile(project.main);
      this.#mainText = { path: project.main, disk: this.#disk, text };
    } catch {
      this.#mainText = null;
    }
  }

  /** Ask for a compile; how soon depends on the engine and the reason. */
  schedule(reason: BuildReason = "edit"): void {
    const target = this.#resolve();
    const engine = target?.engine ?? workspace.engine;
    if (reason === "edit" && engine === "latex") {
      // Disk-based: nothing new to compile until the save lands.
      return;
    }
    this.#scheduler.request(reason === "switch" ? 0 : DEBOUNCE_MS[engine]);
  }

  /** Files changed on disk (a save, git, another editor). */
  noteDiskChange(): void {
    this.#disk += 1;
    this.schedule("disk");
  }

  /** The user agreed to let the engine download missing TeX files once. */
  allowNetwork(): void {
    this.#network = true;
    void this.compile(true);
  }

  async #hasCommand(id: string): Promise<boolean> {
    let known = this.#available[id];
    if (known === undefined) {
      try {
        known = (await kernel.commands()).some((c) => c.id === id);
      } catch {
        known = false;
      }
      this.#available[id] = known;
    }
    return known;
  }

  /** Compile now. `force` skips the "inputs unchanged" shortcut. */
  async compile(force = false): Promise<void> {
    this.#scheduler.cancel();
    const run = ++this.#run;

    const project = workspace.project;
    if (project === null) {
      this.#reset("idle");
      return;
    }
    await this.#loadMainText();
    if (run !== this.#run) return;
    const target = this.#resolve();
    if (target === null) {
      this.#reset("unsupported");
      return;
    }
    const { engine, main } = target;
    this.viaMain = target.viaMain;
    if (!(await this.#hasCommand(COMPILE_COMMAND[engine]))) {
      this.engine = engine;
      this.#reset("unavailable");
      return;
    }

    const sources = engine === "latex" ? {} : documents.dirtySources();
    const key = inputKey(engine, main, sources, this.#disk);
    if (
      !force &&
      key === this.#lastKey &&
      (this.status === "ok" || this.status === "failed")
    ) {
      return;
    }

    this.status = "compiling";
    this.engine = engine;
    try {
      if (engine === "typst") await this.#typst(run, main, project.root, sources);
      else if (engine === "latex") await this.#latex(run, main, project.root);
      else await this.#markdown(run, main);
      if (run === this.#run) this.#lastKey = key;
    } catch (error) {
      if (run !== this.#run) return;
      this.#lastKey = "";
      this.status = "failed";
      this.problems = [];
      const failure =
        engine === "latex"
          ? explainLatexFailure(errorMessage(error))
          : { code: null, title: errorMessage(error), fix: null };
      this.failure = failure;
      this.#write(
        `Compile did not run: ${failure.title}${failure.fix === null ? "" : `. ${failure.fix}`}`,
        "error",
      );
    }
  }

  async #typst(
    run: number,
    main: string,
    root: string,
    sources: Record<string, string>,
  ): Promise<void> {
    const report = await typst.compile({ main, root, sources });
    if (run !== this.#run) return;
    this.#finish(main, fromReport(report), report.ok, report.duration_ms, report.pages);
  }

  async #latex(run: number, main: string, root: string): Promise<void> {
    const report = await latex.compile({
      main: join(root, main),
      ...(this.#network ? { allow_network: true } : {}),
    });
    if (run !== this.#run) return;
    this.#finish(
      main,
      fromLatexReport(report, main),
      report.ok,
      report.duration_ms,
      report.pages ?? this.pages,
    );
  }

  async #markdown(run: number, main: string): Promise<void> {
    const session = documents.all().find((s) => s.path === main);
    const source = session?.text() ?? (await workspace.readFile(main));
    const started = performance.now();
    const rendered = await markdown.renderHtml(source);
    if (run !== this.#run) return;
    this.html = rendered.html;
    this.#finish(
      main,
      fromMarkdown(rendered.diagnostics, main),
      true,
      performance.now() - started,
      1,
    );
  }

  #finish(
    main: string,
    problems: readonly Problem[],
    ok: boolean,
    durationMs: number,
    pages: number,
  ): void {
    this.target = main;
    this.problems = problems;
    this.durationMs = durationMs;
    this.pages = pages;
    this.failure = null;
    if (ok) {
      this.status = "ok";
      this.generation += 1;
      this.#write(
        `Compiled ${main} in ${formatMs(durationMs)} (${pages} ${pages === 1 ? "page" : "pages"}).`,
        "info",
      );
    } else {
      this.status = "failed";
      const { errors } = countProblems(problems);
      this.#write(
        `${main} has ${errors} ${errors === 1 ? "error" : "errors"}; showing the last good pages.`,
        "error",
      );
    }
  }

  #reset(status: BuildStatus): void {
    this.status = status;
    this.problems = [];
    this.pages = 0;
    this.target = "";
    this.failure = null;
    this.#lastKey = "";
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

/** "Compiled in 12 ms", "Compiling", "2 errors", ... for the status bar. */
export function statusText(
  state: Pick<BuildState, "status" | "durationMs" | "counts" | "failure" | "engine">,
): string {
  switch (state.status) {
    case "compiling":
      return "Compiling";
    case "ok":
      return `Compiled in ${formatMs(state.durationMs)}`;
    case "failed": {
      const { errors, warnings } = state.counts;
      if (state.failure !== null) return "Compile did not run";
      if (errors === 0 && warnings === 0) return "Compile failed";
      return `${errors} ${errors === 1 ? "error" : "errors"}`;
    }
    case "unsupported":
      return "No compiler for this file";
    case "unavailable":
      return "No compiler available";
    default:
      return "";
  }
}

export const build = new BuildState();
