/**
 * "Save a version" and "Restore": the flows that ask a question, call the git
 * plugin and tell the person what happened. The rules (counting changes,
 * wording, which files are at risk) are in `workbench/versions.ts`.
 *
 * Every failure is logged with `reportError` and shown as a notice; none is
 * swallowed.
 */

import { kernel, reportError } from "$lib/kernel";
import {
  gitApi,
  type GitIdentity,
  type GitRestored,
  type GitVersionInfo,
} from "$lib/kernel/api";
import { gitActionableError, parseGitError } from "$lib/workbench/git";
import {
  changeSummary,
  changedPaths,
  defaultVersionName,
  planRestore,
  restoredNotice,
  restoreWords,
  type RestoreScope,
} from "$lib/workbench/versions";

import * as actions from "./actions";
import { dialog } from "./dialog.svelte";
import { documents } from "./documents.svelte";
import { notice } from "./notice.svelte";
import { tabs } from "./tabs.svelte";
import { viewers } from "./viewers.svelte";
import { workspace } from "./workspace.svelte";

const api = gitApi(kernel);

function fail(where: string, error: unknown): void {
  reportError(where, error);
  const parsed = parseGitError(error);
  notice.error(gitActionableError(parsed.code, parsed.message));
}

/** Unsaved edits are not on disk yet, so a version would miss them. */
async function saveOpenEdits(): Promise<void> {
  const dirty =
    documents.dirtyPaths.length > 0 ||
    tabs.tabs.some((t) => actions.isTabDirty(t.path));
  if (dirty) await actions.saveAll();
}

/**
 * Make sure git knows who is saving. Asks for a name and email when it does
 * not, and writes them for this project or for all projects. Returns false if
 * the person cancels or the kernel refuses.
 */
export async function ensureIdentity(root: string): Promise<boolean> {
  let current: GitIdentity;
  try {
    current = await api.identityGet({ root });
  } catch (error) {
    fail("reading the author identity", error);
    return false;
  }
  if (current.source !== "none" && current.name !== null && current.email !== null) {
    return true;
  }
  let problem = "";
  for (;;) {
    const answer = await dialog.prompt({
      title: "Your name and email",
      message:
        problem === ""
          ? "Versions carry the name of whoever saved them, so collaborators can tell who did what. It stays on your computer until you share."
          : problem,
      input: { label: "Name", value: current.name ?? "" },
      extraInput: { label: "Email", value: current.email ?? "" },
      choice: {
        label: "Use it for",
        options: [
          { id: "repo", label: "Only for this project" },
          { id: "global", label: "Use for all projects" },
        ],
        value: "repo",
      },
      actions: [{ id: "continue", label: "Continue", primary: true }],
    });
    if (answer.id !== "continue") return false;
    try {
      await api.identitySet({
        root,
        name: answer.value,
        email: answer.extra ?? "",
        global: answer.choice === "global",
      });
      return true;
    } catch (error) {
      const parsed = parseGitError(error);
      if (parsed.code !== "bad-arguments") {
        fail("saving the author identity", error);
        return false;
      }
      reportError("the author identity was refused", error);
      problem = parsed.message;
      current = { ...current, name: answer.value, email: answer.extra ?? "" };
    }
  }
}

/** True once the project is a repository (starting one after a yes). */
async function ensureTracked(root: string): Promise<boolean> {
  const opened = await api.open(root);
  if (opened.is_repo) return true;
  const choice = await dialog.ask({
    title: "Start keeping versions?",
    message:
      "This project is not tracked yet. Manang will start keeping versions of it. Your files are not changed.",
    actions: [{ id: "start", label: "Start keeping versions", primary: true }],
  });
  if (choice !== "start") return false;
  await api.init({ root });
  return true;
}

/** The Save a version command: identity, one name field, then save. */
export async function saveVersion(): Promise<void> {
  const root = workspace.project?.root;
  if (root === undefined) return;
  try {
    if (!(await ensureTracked(root))) return;
    await saveOpenEdits();
    const status = await api.status({ root });
    const count = changedPaths(status.files).length;
    if (count === 0) {
      notice.show(changeSummary(0));
      return;
    }
    if (!(await ensureIdentity(root))) return;
    const answer = await dialog.prompt({
      title: "Save a version",
      message: changeSummary(count),
      input: { label: "Name this version (optional)", value: "" },
      actions: [{ id: "save", label: "Save", primary: true }],
    });
    if (answer.id !== "save") return;
    const name =
      answer.value.trim() === "" ? defaultVersionName(new Date()) : answer.value.trim();
    const saved = await api.saveVersion({ root, name });
    notice.show(`Saved version "${saved.subject}"`);
  } catch (error) {
    const parsed = parseGitError(error);
    if (parsed.code === "nothing-to-save") {
      notice.show(changeSummary(0));
      return;
    }
    fail("saving a version", error);
  }
}

/** What a restore changed on disk reaches open tabs and the file tree. */
function afterRestore(result: GitRestored): void {
  workspace.scheduleRefresh();
  void documents.externalChange(result.restored);
  void viewers.externalChange(result.restored);
  notice.show(restoredNotice(result.restored, result.untouched_new));
}

/**
 * Restore a file (or the whole project) from `version` after a plain
 * confirmation. Current changes to the files it would replace are saved first
 * as a version. Returns the result, or null if cancelled or failed.
 */
export async function restoreFromVersion(
  version: Pick<GitVersionInfo, "hash" | "short" | "subject">,
  scope: RestoreScope,
): Promise<GitRestored | null> {
  const root = workspace.project?.root;
  if (root === undefined) return null;
  try {
    await saveOpenEdits();
    const status = await api.status({ root });
    let plan = planRestore(scope, changedPaths(status.files));
    const ask = async (): Promise<boolean> => {
      const words = restoreWords(scope, version, plan);
      const choice = await dialog.ask({
        title: words.title,
        message: words.message,
        actions: [{ id: "restore", label: words.action, primary: true }],
      });
      return choice === "restore";
    };
    if (!(await ask())) return null;
    if (plan.saveFirst && !(await ensureIdentity(root))) return null;
    const request = {
      root,
      rev: version.hash,
      ...(scope.kind === "whole" ? { whole: true } : { paths: scope.paths }),
    };
    let result: GitRestored;
    try {
      result = await api.restore({ ...request, save_first: plan.saveFirst });
    } catch (error) {
      const parsed = parseGitError(error);
      if (parsed.code !== "dirty-tree" || plan.saveFirst) throw error;
      // Files changed since the list loaded: ask again, now with the safety save.
      plan = { dirty: parsed.paths, saveFirst: true };
      if (!(await ask()) || !(await ensureIdentity(root))) return null;
      result = await api.restore({ ...request, save_first: true });
    }
    afterRestore(result);
    return result;
  } catch (error) {
    fail("restoring a version", error);
    return null;
  }
}
