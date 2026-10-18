/**
 * Self-healing state: the deterministic fixes of the heal plugin for the
 * compile problems on screen, and what happens when one is applied.
 *
 * After every compile the open files with problems are sent to `heal.suggest`
 * (pure, fast; Markdown files are scanned even without a problem). The answer
 * is kept per file. Applying a fix asks again with the text as it is now (the
 * user may have typed since), applies the edits as one undoable step, says what
 * was done with an Undo, and lets the normal recompile run. When that compile
 * still reports the error the fix targeted and no fewer errors, the notice
 * offers Undo instead of claiming success: the verification loop, v1.
 *
 * `heal.auto-apply` applies the certain fixes right after a save.
 */

import { untrack } from "svelte";
import { SvelteSet } from "svelte/reactivity";

import type { FixChoice } from "$lib/editor/diagnostics";
import { setFixRunner } from "$lib/editor/healBridge";
import { inTauri, kernel, reportError } from "$lib/kernel";
import {
  healApi,
  type HealFix,
  type HealFixAll,
  type HealSuggestion,
} from "$lib/kernel/healApi";
import {
  choiceOf,
  diagnosticsFor,
  doneText,
  findingProblem,
  fixesOf,
  noProgressText,
  verdict,
} from "$lib/workbench/heal";
import { countProblems, type Problem } from "$lib/workbench/problems";

import { build } from "./build.svelte";
import { documents } from "./documents.svelte";
import { notice } from "./notice.svelte";
import { settings } from "./settings.svelte";
import { tabs } from "./tabs.svelte";
import { workspace } from "./workspace.svelte";

const api = healApi(kernel);

/** What `heal.suggest` said about one file, with the ids it was asked about. */
interface FileResult {
  readonly suggestion: HealSuggestion;
  /** Problem id per diagnostic index of the request. */
  readonly problemIds: readonly string[];
  /** Problem id per finding index. */
  readonly findingIds: readonly string[];
}

/** A fix just applied, waiting for the recompile that tells whether it worked. */
interface Pending {
  readonly file: string;
  readonly label: string;
  readonly targeted: readonly string[];
  readonly errorsBefore: number;
  /** The fix was saved to disk for the compiler: Undo saves the undone text too. */
  readonly saved: boolean;
}

export class HealState {
  results = $state.raw<Readonly<Record<string, FileResult>>>({});
  /** Problems of text scans (Markdown), listed and marked like compiler problems. */
  problems = $state.raw<readonly Problem[]>([]);

  readonly enabled: boolean = $derived(settings.value("heal.enabled") !== false);
  readonly autoApply: boolean = $derived(settings.value("heal.auto-apply") === true);

  #started = false;
  #run = 0;
  #pending: Pending | null = null;
  #armed: string | null = null;
  #saving = false;

  /** Follow the build and saves. Call once at boot. */
  start(): void {
    if (this.#started) return;
    this.#started = true;
    setFixRunner({ apply: (problemId, fixId) => void this.apply(problemId, fixId) });
    documents.onSave((path) => this.#onSave(path));
    $effect.root(() => {
      $effect(() => {
        void build.problems;
        void build.generation;
        void tabs.active?.id;
        void this.enabled;
        untrack(() => void this.#onBuild());
      });
      // The browser has no shell to push plugin settings.
      $effect(() => {
        const values = {
          "heal.enabled": this.enabled,
          "heal.auto-apply": this.autoApply,
        };
        if (inTauri()) return;
        untrack(() => void api.configure(values).catch(() => undefined));
      });
    });
  }

  /* ----------------------------------------------------------------------- */
  /* Reading                                                                  */
  /* ----------------------------------------------------------------------- */

  /** The fixes of one problem, certain ones first. */
  fixesFor(problem: Problem): readonly HealFix[] {
    if (!this.enabled) return [];
    const result = this.results[problem.file];
    if (result === undefined) return [];
    const at = result.problemIds.indexOf(problem.id);
    const found = result.findingIds.indexOf(problem.id);
    return fixesOf(result.suggestion, at < 0 ? null : at, found < 0 ? null : found);
  }

  choicesFor(problem: Problem): readonly FixChoice[] {
    return this.fixesFor(problem).map(choiceOf);
  }

  /** The combined certain fixes of a file, when there are at least two. */
  fixAllFor(file: string): HealFixAll | null {
    if (!this.enabled) return null;
    return this.results[file]?.suggestion.fix_all ?? null;
  }

  /* ----------------------------------------------------------------------- */
  /* Asking                                                                   */
  /* ----------------------------------------------------------------------- */

  async #onBuild(): Promise<void> {
    this.#verify();
    await this.refresh();
    await this.#autoApply();
  }

  /** The text of `file`: the open document's, else what is on disk. */
  async #textOf(file: string): Promise<string | null> {
    const session = documents.get(file);
    if (session !== undefined) return session.text();
    try {
      return await workspace.readFile(file);
    } catch {
      return null;
    }
  }

  /** The compiler problems of `file` that can have a fix. */
  #compileProblems(file: string): Problem[] {
    return build.problems.filter(
      (p) => p.file === file && p.source === undefined && p.severity !== "info",
    );
  }

  async #suggest(file: string, text: string): Promise<FileResult | null> {
    const problems = this.#compileProblems(file);
    try {
      const suggestion = await api.suggest({
        engine: workspace.engineFor(file),
        file,
        text,
        diagnostics: diagnosticsFor(problems),
      });
      const findingIds = suggestion.findings.map((f) => findingProblem(file, f, 1).id);
      return { suggestion, problemIds: problems.map((p) => p.id), findingIds };
    } catch (error) {
      reportError("heal.suggest", error);
      return null;
    }
  }

  /** Ask for the fixes of every file with problems (and the shown Markdown file). */
  async refresh(): Promise<void> {
    const run = ++this.#run;
    if (!this.enabled) {
      this.results = {};
      this.problems = [];
      return;
    }
    const files = new SvelteSet<string>();
    for (const p of build.problems) {
      if (p.file !== "" && p.source === undefined && p.severity !== "info") {
        files.add(p.file);
      }
    }
    const active = tabs.active;
    if (
      active?.kind === "document" &&
      active.view === "text" &&
      workspace.engineFor(active.path) === "markdown"
    ) {
      files.add(active.path);
    }
    const results: Record<string, FileResult> = {};
    const scanned: Problem[] = [];
    for (const file of files) {
      const text = await this.#textOf(file);
      if (text === null) continue;
      const result = await this.#suggest(file, text);
      if (run !== this.#run) return;
      if (result === null) continue;
      results[file] = result;
      for (const finding of result.suggestion.findings) {
        scanned.push(findingProblem(file, finding, 1));
      }
    }
    if (run !== this.#run) return;
    this.results = results;
    this.problems = scanned;
  }

  /* ----------------------------------------------------------------------- */
  /* Applying                                                                 */
  /* ----------------------------------------------------------------------- */

  #problemById(id: string): Problem | undefined {
    return [...build.problems, ...this.problems].find((p) => p.id === id);
  }

  /** Apply the fix `fixId` of the problem `problemId`. */
  async apply(problemId: string, fixId: string): Promise<boolean> {
    const problem = this.#problemById(problemId);
    if (problem === undefined) {
      notice.error("That problem is no longer there. Compile again.");
      return false;
    }
    const session = documents.get(problem.file);
    if (session === undefined) {
      notice.error(`Open ${problem.file} to fix it.`);
      return false;
    }
    // Ask again with the text as it is now: the user may have typed since.
    const fresh = await this.#suggest(problem.file, session.text());
    const fix = fresh?.suggestion.fixes.find((f) => f.id === fixId);
    if (fix === undefined) {
      notice.error("That fix no longer applies. The text changed.");
      void this.refresh();
      return false;
    }
    const targeted = fix.diagnostics
      .map((i) => this.#compileProblems(problem.file)[i]?.message)
      .filter((m): m is string => m !== undefined);
    return this.#commit(
      problem.file,
      fix.label,
      doneText(fix.label),
      fix.edits,
      targeted,
    );
  }

  /** Apply the combined certain fixes of `file` as one step. */
  async applyAll(file: string): Promise<boolean> {
    const session = documents.get(file);
    if (session === undefined) {
      notice.error(`Open ${file} to fix it.`);
      return false;
    }
    const fresh = await this.#suggest(file, session.text());
    const all = fresh?.suggestion.fix_all;
    if (all === undefined) {
      notice.error("Those fixes no longer apply. The text changed.");
      void this.refresh();
      return false;
    }
    const problems = this.#compileProblems(file);
    const targeted = (fresh?.suggestion.fixes ?? [])
      .filter((f) => all.fixes.includes(f.id))
      .flatMap((f) => f.diagnostics)
      .map((i) => problems[i]?.message)
      .filter((m): m is string => m !== undefined);
    const n = all.fixes.length;
    return this.#commit(file, all.label, `Applied ${n} fixes.`, all.edits, targeted);
  }

  async #commit(
    file: string,
    label: string,
    done: string,
    edits: HealFix["edits"],
    targeted: readonly string[],
  ): Promise<boolean> {
    const session = documents.get(file);
    if (session === undefined) return false;
    const wasDirty = session.dirty;
    if (!session.applyEdits(edits)) {
      notice.error("That fix no longer applies. The text changed.");
      return false;
    }
    // LaTeX compiles what is on disk. Save only when that writes nothing but
    // the fix; otherwise the person saves, as they would after any edit.
    const saves = workspace.engineFor(file) === "latex" && !wasDirty;
    const pending: Pending = {
      file,
      label,
      targeted,
      errorsBefore: countProblems(build.problems).errors,
      saved: saves,
    };
    this.#pending = pending;
    const undo = { label: "Undo", run: () => this.undo(pending) };
    const needsSave = workspace.engineFor(file) === "latex" && wasDirty;
    notice.show(needsSave ? `${done} Save to compile.` : done, "info", undo);
    if (saves) await this.#save(file);
    return true;
  }

  async #save(file: string): Promise<void> {
    this.#saving = true;
    try {
      await documents.save(file);
    } finally {
      this.#saving = false;
    }
  }

  /** Undo the fix of `pending` (one history step), and save again if the fix was saved. */
  undo(pending: Pending): void {
    this.#pending = null;
    const session = documents.get(pending.file);
    if (session === undefined || !session.undoLast()) {
      notice.error("Nothing to undo.");
      return;
    }
    notice.show("Fix undone.");
    if (pending.saved) void this.#save(pending.file);
  }

  /* ----------------------------------------------------------------------- */
  /* Verifying and automatic fixes                                            */
  /* ----------------------------------------------------------------------- */

  /** The recompile after a fix is in: did it work? */
  #verify(): void {
    const pending = this.#pending;
    if (pending === null || build.status === "compiling") return;
    this.#pending = null;
    if (verdict(pending.targeted, pending.errorsBefore, build.problems) === "worked")
      return;
    notice.show(noProgressText(pending.label), "info", {
      label: "Undo",
      run: () => this.undo(pending),
    });
  }

  #onSave(path: string): void {
    if (this.#saving || !this.autoApply || !this.enabled) return;
    this.#armed = path;
    // Typst and Markdown already compiled the text; LaTeX compiles after the
    // save, and the build effect picks it up.
    if (workspace.engineFor(path) !== "latex") void this.#onBuild();
  }

  /** After a save with `heal.auto-apply` on: apply the certain fixes once. */
  async #autoApply(): Promise<void> {
    const file = this.#armed;
    if (file === null || build.status === "compiling") return;
    this.#armed = null;
    const result = this.results[file];
    if (result === undefined) return;
    const all = result.suggestion.fix_all;
    if (all !== undefined) {
      await this.applyAll(file);
      return;
    }
    const certain = result.suggestion.fixes.find((f) => f.confidence === "certain");
    if (certain === undefined) return;
    const problemId = certain.diagnostics
      .map((i) => result.problemIds[i])
      .find((id) => id !== undefined);
    const findingId =
      certain.finding === undefined ? undefined : result.findingIds[certain.finding];
    const id = problemId ?? findingId;
    if (id !== undefined) await this.apply(id, certain.id);
  }
}

export const heal = new HealState();
